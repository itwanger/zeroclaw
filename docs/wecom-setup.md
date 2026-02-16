# 企业微信 (WeCom) 接入指南

本指南将帮助您将 ZeroClaw 接入企业微信,实现通过企业微信与 AI 助手对话。

## 📋 前置准备

### 1. 企业微信管理员权限

您需要有企业微信管理后台的权限,或联系管理员帮助创建自建应用。

### 2. 本地开发工具（测试阶段）

如果您要在本地测试,需要安装内网穿透工具:

```bash
# 使用 ngrok (推荐)
brew install ngrok

# 或者使用其他内网穿透工具如 localtunnel, frp 等
```

## 🚀 接入步骤

### 步骤 1: 在企业微信后台创建自建应用

1. 登录[企业微信管理后台](https://work.weixin.qq.com/)
2. 进入 **应用管理 → 自建应用**
3. 点击 **创建应用**
4. 填写应用信息:
   - 应用名称: `ZeroClaw AI 助手`
   - 应用 Logo: (可选)
   - 可见范围: 选择需要使用的成员
5. 创建完成后,记录以下信息:
   - **AgentId** (应用 ID)
   - **Secret** (应用密钥)

### 步骤 2: 获取企业 ID (CorpID)

在企业微信管理后台首页,找到 **我的企业** → **企业信息**,复制 **企业 ID (CorpID)**。

### 步骤 3: 开启智能机器人功能

1. 在应用详情页,找到 **智能机器人** 功能
2. 点击 **启用**
3. 记录 **AIBotID** (机器人 ID)

### 步骤 4: 生成加密参数

企业微信需要两个自定义参数:

#### 4.1 Token (自定义)

任意字符串,3-32 位字母数字组合:

```bash
# 示例
export WECOM_TOKEN="my_secret_token_2024"
```

#### 4.2 EncodingAESKey (随机生成)

在企业微信后台的回调配置页面:

1. 点击 **随机生成** 按钮
2. 复制生成的 **43 位 Base64 字符串**
3. 保存到环境变量:

```bash
export WECOM_ENCODING_KEY="abcdefghijk...XYZ123"  # 43 chars
```

### 步骤 5: 启动 ngrok (本地测试)

```bash
# 启动 ngrok,映射到 ZeroClaw gateway 端口 (默认 8080)
ngrok http 8080
```

ngrok 会输出一个 HTTPS URL,例如:

```
Forwarding  https://abc123def456.ngrok.io -> http://localhost:8080
```

**复制这个 HTTPS URL**,稍后需要填入企业微信后台。

### 步骤 6: 配置 ZeroClaw

编辑 `~/.zeroclaw/config.toml`,添加企业微信配置:

```toml
[channels.wecom]
# 从企业微信后台获取
corpid = "ww1234567890abcdef"           # 企业 ID
secret = "your-application-secret"      # 应用 Secret
aibotid = "your-ai-bot-id"              # 智能机器人 ID

# 您自己定义的参数
token = "my_secret_token_2024"          # 自定义 Token (3-32 位)
encoding_aes_key = "abcdefghijk...XYZ"  # 企业微信后台生成的 43 位密钥

# 回调 URL (ngrok 提供的 HTTPS 地址 + 固定路径)
callback_url = "https://abc123def456.ngrok.io/wecom/callback"

# 可选: 白名单 (限制哪些用户可以使用)
allowed_users = ["*"]  # "*" = 允许所有人,或填具体 UserID: ["user1", "user2"]
```

### 步骤 7: 启动 ZeroClaw Gateway

```bash
zeroclaw gateway
```

Gateway 会在 `http://localhost:8080` 监听,ngrok 会将公网请求转发到这里。

### 步骤 8: 在企业微信后台配置回调 URL

1. 返回企业微信应用管理页面
2. 找到 **接收消息** → **设置 API 接收**
3. 填写以下信息:
   - **URL**: `https://abc123def456.ngrok.io/wecom/callback` (你的 ngrok URL + `/wecom/callback`)
   - **Token**: `my_secret_token_2024` (与 config.toml 一致)
   - **EncodingAESKey**: `abcdefghijk...XYZ` (与 config.toml 一致)
4. 点击 **保存**

企业微信会立即发送验证请求到您的 callback URL。如果配置正确,会显示 **验证成功**。

### 步骤 9: 测试对话

1. 在企业微信移动端或桌面端,找到您创建的应用
2. 进入应用,给机器人发送消息: `你好`
3. ZeroClaw 会接收消息并回复

您应该会看到 gateway 的日志输出:

```
💬 [WeCom] from user123: 你好
⏳ Processing message...
🤖 Reply (1234ms): 你好!我是 ZeroClaw AI 助手...
```

## 🔍 故障排查

### 问题 1: 回调 URL 验证失败

**原因**: Token 或 EncodingAESKey 不匹配,或 ngrok 未正确转发。

**解决**:
1. 检查 `config.toml` 中的 `token` 和 `encoding_aes_key` 与企业微信后台一致
2. 确认 ngrok 正在运行,并且 URL 正确
3. 查看 gateway 日志是否收到验证请求

### 问题 2: 收不到消息

**原因**: 企业微信未将消息转发到回调 URL。

**解决**:
1. 确认回调 URL 验证成功
2. 检查应用的 **可见范围** 是否包含您的账号
3. 确认智能机器人功能已启用

### 问题 3: ngrok URL 过期

**原因**: ngrok 免费版每次重启 URL 会变。

**解决**:
- 每次重启 ngrok 后,需要更新 `config.toml` 中的 `callback_url` 并重新在企业微信后台配置
- 升级 ngrok 付费版获得固定域名
- 或直接部署到公网服务器 (见生产部署章节)

### 问题 4: 消息发送失败

**原因**: access_token 无效或过期。

**解决**:
1. 运行健康检查: `zeroclaw channel doctor`
2. 检查 `corpid` 和 `secret` 是否正确
3. 确认应用未被停用

## 🌐 生产环境部署

在生产环境,您需要:

1. **公网服务器**: 需要一台有公网 IP 的服务器 (如阿里云、腾讯云)
2. **域名和 HTTPS 证书**: 企业微信强制要求 HTTPS
   ```bash
   # 使用 Let's Encrypt 自动获取证书
   certbot --nginx -d your-domain.com
   ```
3. **配置固定 callback_url**:
   ```toml
   callback_url = "https://your-domain.com/wecom/callback"
   ```
4. **使用 systemd 或 supervisor 管理服务**:
   ```bash
   # 创建 systemd service
   sudo systemctl enable zeroclaw-gateway
   sudo systemctl start zeroclaw-gateway
   ```

## 📚 配置示例

### 最小配置 (测试)

```toml
[channels.wecom]
corpid = "ww1234567890abcdef"
secret = "your-secret"
aibotid = "your-bot-id"
token = "test_token_123"
encoding_aes_key = "7oCvxzgCP3d3RLzzfhitAz2aiG3HyprpiVSDeH3W4bQ"
callback_url = "https://abc123.ngrok.io/wecom/callback"
allowed_users = ["*"]
```

### 完整配置 (生产)

```toml
[channels.wecom]
corpid = "ww1234567890abcdef"
secret = "your-application-secret-from-wecom"
aibotid = "your-aibot-id-from-wecom"
token = "production_secure_token_2024"
encoding_aes_key = "7oCvxzgCP3d3RLzzfhitAz2aiG3HyprpiVSDeH3W4bQ"
callback_url = "https://zeroclaw.yourdomain.com/wecom/callback"

# 限制只有特定用户可以使用 (企业微信 UserID)
allowed_users = ["zhangsan", "lisi", "wangwu"]
```

## 🔐 安全建议

1. **不要将 `secret` 和 `token` 提交到 Git**
2. **使用环境变量管理敏感信息**:
   ```bash
   export WECOM_SECRET="your-secret"
   export WECOM_TOKEN="your-token"
   ```
3. **生产环境使用 `allowed_users` 白名单**
4. **定期轮换 `secret` (企业微信后台可重新生成)**

## 💡 下一步

- 查看 [AGENTS.md](../AGENTS.md) 了解如何自定义 AI 助手行为
- 查看 [TOOLS.md](../TOOLS.md) 了解可用工具列表
- 加入社区讨论: [GitHub Discussions](https://github.com/theonlyhennygod/zeroclaw/discussions)

---

遇到问题?欢迎提交 [Issue](https://github.com/theonlyhennygod/zeroclaw/issues) 或查看 [FAQ](./faq.md)!
