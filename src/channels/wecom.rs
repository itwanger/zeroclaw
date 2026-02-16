use super::traits::{Channel, ChannelMessage};
use super::wecom_crypto::WeComCrypto;
use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

/// WeCom (Enterprise WeChat) channel
/// 
/// Receives messages via webhook callback (POST /wecom/callback)
/// Sends messages via WeCom Bot API
pub struct WeComChannel {
    corpid: String,
    secret: String,
    aibotid: String,
    token: String,
    crypto: WeComCrypto,
    allowed_users: Vec<String>,
    client: reqwest::Client,
    /// Access token cache (expires every 2 hours)
    access_token_cache: Arc<Mutex<Option<AccessTokenCache>>>,
}

#[derive(Clone)]
struct AccessTokenCache {
    token: String,
    expires_at: std::time::Instant,
}

impl WeComChannel {
    pub fn new(
        corpid: String,
        secret: String,
        aibotid: String,
        token: String,
        encoding_aes_key: String,
        allowed_users: Vec<String>,
    ) -> Result<Self> {
        let crypto = WeComCrypto::new(&encoding_aes_key, &corpid)?;

        Ok(Self {
            corpid,
            secret,
            aibotid,
            token,
            crypto,
            allowed_users,
            client: reqwest::Client::new(),
            access_token_cache: Arc::new(Mutex::new(None)),
        })
    }

    /// Get or refresh access_token
    async fn get_access_token(&self) -> Result<String> {
        let mut cache = self.access_token_cache.lock().await;

        // Check cache validity
        if let Some(cached) = cache.as_ref() {
            if cached.expires_at > std::time::Instant::now() {
                return Ok(cached.token.clone());
            }
        }

        // Fetch new token
        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={}&corpsecret={}",
            self.corpid, self.secret
        );

        let resp: GetTokenResponse = self.client.get(&url).send().await?.json().await?;

        if resp.errcode != 0 {
            bail!("WeCom gettoken failed: {} ({})", resp.errmsg, resp.errcode);
        }

        let token = resp.access_token.ok_or_else(|| {
            anyhow::anyhow!("WeCom gettoken returned no access_token")
        })?;

        // Cache for 7000 seconds (token valid for 7200s, keep 200s buffer)
        *cache = Some(AccessTokenCache {
            token: token.clone(),
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(7000),
        });

        Ok(token)
    }

    fn is_user_allowed(&self, userid: &str) -> bool {
        self.allowed_users.iter().any(|u| u == "*" || u == userid)
    }

    /// Verify callback URL signature (for WeCom initial verification)
    pub fn verify_callback(
        &self,
        msg_signature: &str,
        timestamp: &str,
        nonce: &str,
        echostr: &str,
    ) -> Result<String> {
        // Verify signature
        if !WeComCrypto::verify_signature(&self.token, timestamp, nonce, echostr, msg_signature)? {
            bail!("Signature verification failed");
        }

        // Decrypt echostr and return
        self.crypto.decrypt(echostr)
    }

    /// Parse incoming encrypted message from WeCom callback
    pub fn parse_callback_message(
        &self,
        msg_signature: &str,
        timestamp: &str,
        nonce: &str,
        encrypted_xml: &str,
    ) -> Result<IncomingMessage> {
        // Extract <Encrypt> from XML
        let encrypt = extract_xml_tag(encrypted_xml, "Encrypt")
            .ok_or_else(|| anyhow::anyhow!("Missing <Encrypt> in callback XML"))?;

        // Verify signature
        if !WeComCrypto::verify_signature(&self.token, timestamp, nonce, &encrypt, msg_signature)? {
            bail!("Signature verification failed");
        }

        // Decrypt message
        let decrypted = self.crypto.decrypt(&encrypt)?;

        // Parse decrypted XML
        let msg = parse_wecom_message(&decrypted)?;

        Ok(msg)
    }

    /// Build encrypted XML response for WeCom callback
    pub fn build_encrypted_response(&self, content: &str, timestamp: &str, nonce: &str) -> Result<String> {
        use sha2::{Digest, Sha256};

        let encrypted = self.crypto.encrypt(content)?;

        // Build signature
        let mut items = vec![&self.token, timestamp, nonce, &encrypted];
        items.sort_unstable();
        let concat = items.join("");
        let mut hasher = Sha256::new();
        hasher.update(concat.as_bytes());
        let signature = hex::encode(hasher.finalize());

        // Build XML response
        let xml = format!(
            r#"<xml>
<Encrypt><![CDATA[{encrypted}]]></Encrypt>
<MsgSignature><![CDATA[{signature}]]></MsgSignature>
<TimeStamp>{timestamp}</TimeStamp>
<Nonce><![CDATA[{nonce}]]></Nonce>
</xml>"#
        );

        Ok(xml)
    }
}

#[async_trait]
impl Channel for WeComChannel {
    fn name(&self) -> &str {
        "WeCom"
    }

    async fn send(&self, message: &str, recipient: &str) -> Result<()> {
        let access_token = self.get_access_token().await?;

        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={}",
            access_token
        );

        let payload = SendMessageRequest {
            touser: recipient.to_string(),
            msgtype: "text".to_string(),
            agentid: self.aibotid.clone(),
            text: TextContent {
                content: message.to_string(),
            },
            safe: 0,
        };

        let resp: SendMessageResponse = self.client.post(&url).json(&payload).send().await?.json().await?;

        if resp.errcode != 0 {
            bail!("WeCom send message failed: {} ({})", resp.errmsg, resp.errcode);
        }

        tracing::info!("WeCom message sent to {recipient}");
        Ok(())
    }

    async fn listen(&self, _tx: tokio::sync::mpsc::Sender<ChannelMessage>) -> Result<()> {
        // WeCom uses webhook-based message delivery (handled in gateway)
        // This method should not be called directly for WeCom
        bail!(
            "WeCom channel uses webhook-based delivery. \
             Start the gateway with `zeroclaw gateway` and configure callback URL in WeCom admin."
        );
    }

    async fn health_check(&self) -> bool {
        // Try to fetch access_token
        match self.get_access_token().await {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("WeCom health check failed: {e}");
                false
            }
        }
    }
}

// ── API Types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GetTokenResponse {
    errcode: i32,
    errmsg: String,
    access_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    touser: String,
    msgtype: String,
    agentid: String,
    text: TextContent,
    safe: i32,
}

#[derive(Debug, Serialize)]
struct TextContent {
    content: String,
}

#[derive(Debug, Deserialize)]
struct SendMessageResponse {
    errcode: i32,
    errmsg: String,
}

#[derive(Debug)]
pub struct IncomingMessage {
    pub from_userid: String,
    pub msg_type: String,
    pub content: String,
    pub msg_id: String,
}

// ── XML Parsing Helpers ──────────────────────────────────────────

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start = format!("<{tag}><![CDATA[");
    let end = format!("]]></{tag}>");

    let start_idx = xml.find(&start)? + start.len();
    let end_idx = xml[start_idx..].find(&end)? + start_idx;

    Some(xml[start_idx..end_idx].to_string())
}

fn parse_wecom_message(xml: &str) -> Result<IncomingMessage> {
    let from_userid = extract_xml_tag(xml, "FromUserName")
        .ok_or_else(|| anyhow::anyhow!("Missing FromUserName"))?;
    let msg_type = extract_xml_tag(xml, "MsgType")
        .ok_or_else(|| anyhow::anyhow!("Missing MsgType"))?;
    let content = extract_xml_tag(xml, "Content").unwrap_or_default();
    let msg_id = extract_xml_tag(xml, "MsgId").unwrap_or_default();

    Ok(IncomingMessage {
        from_userid,
        msg_type,
        content,
        msg_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_xml_tag_works() {
        let xml = r#"<xml><Encrypt><![CDATA[encrypted_data]]></Encrypt></xml>"#;
        assert_eq!(
            extract_xml_tag(xml, "Encrypt"),
            Some("encrypted_data".to_string())
        );
    }

    #[test]
    fn parse_wecom_message_works() {
        let xml = r#"<xml>
<FromUserName><![CDATA[user123]]></FromUserName>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[Hello bot]]></Content>
<MsgId><![CDATA[1234567890]]></MsgId>
</xml>"#;

        let msg = parse_wecom_message(xml).unwrap();
        assert_eq!(msg.from_userid, "user123");
        assert_eq!(msg.msg_type, "text");
        assert_eq!(msg.content, "Hello bot");
        assert_eq!(msg.msg_id, "1234567890");
    }
}
