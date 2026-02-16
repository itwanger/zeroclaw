use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// DingTalk API client for Stream mode and message sending
pub struct DingTalkApi {
    client_id: String,
    client_secret: String,
    client: reqwest::Client,
    /// Cached access token
    access_token: Option<AccessTokenCache>,
}

#[derive(Clone)]
struct AccessTokenCache {
    token: String,
    expires_at: Instant,
}

// ── API Request/Response Types ───────────────────────────────────

#[derive(Debug, Deserialize)]
struct GetTokenResponse {
    errcode: i32,
    errmsg: String,
    access_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Debug, Serialize)]
struct OpenConnectionRequest {
    #[serde(rename = "clientId")]
    client_id: String,
    #[serde(rename = "clientSecret")]
    client_secret: String,
    subscriptions: Vec<SubscriptionItem>,
}

#[derive(Debug, Serialize)]
struct SubscriptionItem {
    #[serde(rename = "type")]
    sub_type: String,
    topic: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenConnectionResponse {
    pub endpoint: String,
    pub ticket: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamMessage {
    #[serde(rename = "specVersion")]
    pub spec_version: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub headers: MessageHeaders,
    pub data: String, // JSON string
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageHeaders {
    #[serde(rename = "messageId")]
    pub message_id: String,
    #[serde(rename = "eventId")]
    pub event_id: Option<String>,
    #[serde(rename = "eventType")]
    pub event_type: Option<String>,
    #[serde(rename = "eventBornTime")]
    pub event_born_time: Option<i64>,
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RobotMessage {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
    #[serde(rename = "atUsers")]
    pub at_users: Option<Vec<AtUser>>,
    #[serde(rename = "chatbotCorpId")]
    pub chatbot_corp_id: Option<String>,
    #[serde(rename = "chatbotUserId")]
    pub chatbot_user_id: String,
    #[serde(rename = "msgId")]
    pub msg_id: String,
    #[serde(rename = "senderNick")]
    pub sender_nick: String,
    #[serde(rename = "senderStaffId")]
    pub sender_staff_id: String,
    #[serde(rename = "sessionWebhook")]
    pub session_webhook: String,
    pub text: TextContent,
    #[serde(rename = "msgtype")]
    pub msg_type: String,
    #[serde(rename = "createAt")]
    pub create_at: i64,
    #[serde(rename = "conversationType")]
    pub conversation_type: String,
    #[serde(rename = "senderId")]
    pub sender_id: String,
    #[serde(rename = "conversationTitle")]
    pub conversation_title: Option<String>,
    #[serde(rename = "isAdmin")]
    pub is_admin: Option<bool>,
    #[serde(rename = "sessionWebhookExpiredTime")]
    pub session_webhook_expired_time: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AtUser {
    #[serde(rename = "dingtalkId")]
    pub dingtalk_id: String,
    #[serde(rename = "staffId")]
    pub staff_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TextContent {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct AckMessage {
    pub code: i32,
    pub message: String,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendMessageRequest {
    #[serde(rename = "msgtype")]
    pub msg_type: String,
    pub text: SendTextContent,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendTextContent {
    pub content: String,
}

// ── Implementation ───────────────────────────────────────────────

impl DingTalkApi {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            client: reqwest::Client::new(),
            access_token: None,
        }
    }

    /// Get or refresh access token
    pub async fn get_access_token(&mut self) -> Result<String> {
        // Check cache
        if let Some(ref cache) = self.access_token {
            if cache.expires_at > Instant::now() {
                return Ok(cache.token.clone());
            }
        }

        // Fetch new token
        let url = format!(
            "https://oapi.dingtalk.com/gettoken?appkey={}&appsecret={}",
            self.client_id, self.client_secret
        );

        let resp: GetTokenResponse = self.client.get(&url).send().await?.json().await?;

        if resp.errcode != 0 {
            bail!("Failed to get access token: {} ({})", resp.errmsg, resp.errcode);
        }

        let token = resp.access_token.ok_or_else(|| {
            anyhow::anyhow!("No access_token in response")
        })?;

        // Cache token (expires_in - 60s buffer)
        let expires_in = resp.expires_in.unwrap_or(7200);
        let expires_at = Instant::now() + Duration::from_secs(expires_in.saturating_sub(60));

        self.access_token = Some(AccessTokenCache {
            token: token.clone(),
            expires_at,
        });

        Ok(token)
    }

    /// Open WebSocket connection and get endpoint + ticket
    pub async fn open_connection(&mut self) -> Result<OpenConnectionResponse> {
        let access_token = self.get_access_token().await?;

        let req = OpenConnectionRequest {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            subscriptions: vec![
                // Subscribe to robot messages (both single chat and group chat)
                SubscriptionItem {
                    sub_type: "CALLBACK".to_string(),
                    topic: "/v1.0/im/bot/messages/get".to_string(),
                },
                // Subscribe to all events as fallback
                SubscriptionItem {
                    sub_type: "EVENT".to_string(),
                    topic: "*".to_string(),
                },
            ],
        };

        let url = "https://api.dingtalk.com/v1.0/gateway/connections/open";

        let resp = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await?;
            bail!("Failed to open connection: {} - {}", status, body);
        }

        let conn: OpenConnectionResponse = resp.json().await?;
        Ok(conn)
    }

    /// Send message via session webhook (Stream mode)
    pub async fn send_message_via_webhook(&self, webhook: &str, content: &str) -> Result<()> {
        let msg = SendMessageRequest {
            msg_type: "text".to_string(),
            text: SendTextContent {
                content: content.to_string(),
            },
        };

        let resp = self.client.post(webhook).json(&msg).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await?;
            bail!("Failed to send message: {} - {}", status, body);
        }

        Ok(())
    }

    /// Parse robot message from Stream data
    pub fn parse_robot_message(&self, data: &str) -> Result<RobotMessage> {
        let msg: RobotMessage = serde_json::from_str(data)?;
        Ok(msg)
    }

    /// Build ACK response for Stream message
    pub fn build_ack(success: bool) -> AckMessage {
        if success {
            AckMessage {
                code: 200,
                message: "OK".to_string(),
                data: None,
            }
        } else {
            AckMessage {
                code: 500,
                message: "Internal Error".to_string(),
                data: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_creation() {
        let api = DingTalkApi::new("test_id".into(), "test_secret".into());
        assert_eq!(api.client_id, "test_id");
        assert_eq!(api.client_secret, "test_secret");
    }

    #[test]
    fn test_build_ack() {
        let ack_ok = DingTalkApi::build_ack(true);
        assert_eq!(ack_ok.code, 200);

        let ack_err = DingTalkApi::build_ack(false);
        assert_eq!(ack_err.code, 500);
    }
}
