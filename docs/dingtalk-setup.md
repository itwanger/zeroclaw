# 钉钉 Stream 模式接入指南

本指南将帮助您将 ZeroClaw 接入钉钉 Stream 模式，实现**本地开发，无需公网 IP** 的 AI 机器人。

## 🎯 核心优势

| 特性 | Stream 模式 | Webhook 模式 |
|------|------------|--------------|
| **公网 IP/域名** | ✅ 不需要 | ❌ 必需 |
| **备案要求** | ✅ 不需要 | ❌ 需要 |
| **内网穿透** | ✅ 不需要 | ❌ 需要 ngrok |
| **本地开发** | ✅ 完美支持 | ❌ 配置麻烦 |
| **安全性** | ✅ TLS 加密反向连接 | ⚠️ 暴露服务 |

## 📋 准备工作

### 1. 钉钉开发者账号

- 登录 [钉钉开放平台](https://open-dev.dingtalk.com/)
- 确保您有权限创建企业内部应用

### 2. 创建企业内部应用

1. 进入 **应用开发 → 企业内部开发 → 创建应用**
2. 填写应用信息：
   - 应用名称：`ZeroClaw AI 助手`
   - 应用描述：AI 智能对话机器人
   - 开发方式：**企业内部自主开发**

3. 创建完成后，记录以下信息：
   - **Client ID** (原 AppKey)
   - **Client Secret** (原 AppSecret)

### 3. 配置机器人能力

在应用管理页面：

1. 点击 **机器人与消息** → **添加机器人**
2. 配置机器人信息：
   - 机器人名称：`ZeroClaw`
   - 消息接收模式：选择 **Stream 模式** ✅
   - 机器人权限：勾选 **接收群聊消息** 和 **发送消息**

## 🚀 配置 ZeroClaw

### 步骤 1: 编辑配置文件

打开 `~/.zeroclaw/config.toml`，添加钉钉配置：

```toml
[channels_config.dingtalk]
client_id = "dingxxxxxxxxxxxxxx"        # 您的 Client ID
client_secret = "your-client-secret"    # 您的 Client Secret
allowed_users = ["*"]                   # "*" 允许所有人，或指定员工 ID
```

### 步骤 2: 启动 ZeroClaw

**重要**: 直接启动 channel，不需要 gateway！

```bash
cd /Users/itwanger/Documents/GitHub/zeroclaw
cargo run --release -- channel start
```

您会看到类似输出：

```
🦀 ZeroClaw Channel Server
  🤖 Model:    glm-5
  🧠 Memory:   sqlite (auto-save: on)
  📡 Channels: DingTalk

  Listening for messages... (Ctrl+C to stop)

Opening DingTalk Stream connection...
Got endpoint: wss://stream.dingtalk.com/...
Connecting to WebSocket...
WebSocket connected successfully
DingTalk Stream client starting...
```

### 步骤 3: 测试机器人

1. 在钉钉移动端或桌面端，找到您的应用
2. 添加机器人到群聊，或直接与机器人单聊
3. 发送消息：`你好`
4. 等待 AI 回复

## 📊 工作原理

```
您的电脑 (本地)
    ↓
ZeroClaw 主动连接钉钉服务器 (WebSocket over TLS)
    ↓
钉钉通过这个连接推送消息
    ↓
ZeroClaw 处理并通过同一连接回复
```

**关键点**：
- ✅ 您的应用**主动连接**钉钉，而不是钉钉连接您
- ✅ 不需要暴露本地端口
- ✅ 不需要 ngrok 等内网穿透工具
- ✅ TLS 加密，安全可靠

## 🔧 高级配置

### 限制特定用户

只允许特定员工使用机器人：

```toml
[channels_config.dingtalk]
client_id = "dingxxxxxx"
client_secret = "your-secret"
allowed_users = ["员工ID1", "员工ID2", "员工ID3"]
```

**如何获取员工 ID**：
1. 在钉钉管理后台 → 通讯录
2. 找到员工信息，查看 `staffId` 或 `userId`

### 同时配置多个 Channel

您可以同时配置钉钉、企业微信等多个渠道：

```toml
[channels_config.dingtalk]
client_id = "your-dingtalk-id"
client_secret = "your-dingtalk-secret"
allowed_users = ["*"]

[channels_config.wecom]
corpid = "your-wecom-corpid"
secret = "your-wecom-secret"
# ... 其他配置
```

启动时会同时监听所有配置的 channel。

## 🔍 故障排查

### 问题 1: 连接失败 "Failed to get access token"

**原因**: Client ID 或 Client Secret 错误

**解决**:
1. 检查配置文件中的凭证是否正确
2. 确认凭证未过期
3. 在钉钉开放平台重新生成凭证

### 问题 2: WebSocket 连接超时

**原因**: 网络问题或防火墙拦截

**解决**:
1. 检查网络连接
2. 确认可以访问 `wss://stream.dingtalk.com`
3. 检查防火墙/代理设置

### 问题 3: 收到消息但无响应

**原因**: 权限不足或消息格式错误

**解决**:
1. 确认机器人有 **发送消息** 权限
2. 查看 ZeroClaw 日志是否有错误
3. 运行健康检查：`zeroclaw channel doctor`

### 问题 4: "Unauthorized user"

**原因**: 用户不在 `allowed_users` 白名单中

**解决**:
- 将 `allowed_users` 设置为 `["*"]` 允许所有人
- 或添加该用户的 staffId 到白名单

## 📚 命令参考

```bash
# 启动 channel 监听
zeroclaw channel start

# 查看配置的 channel 列表
zeroclaw channel list

# 检查 channel 健康状态
zeroclaw channel doctor

# 查看日志（实时）
zeroclaw channel start 2>&1 | tee dingtalk.log
```

## 🎯 生产环境部署

### 使用 systemd 管理服务

创建服务文件 `/etc/systemd/system/zeroclaw-dingtalk.service`:

```ini
[Unit]
Description=ZeroClaw DingTalk Channel
After=network.target

[Service]
Type=simple
User=your-username
WorkingDirectory=/home/your-username/zeroclaw
ExecStart=/home/your-username/zeroclaw/target/release/zeroclaw channel start
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

启动服务：

```bash
sudo systemctl daemon-reload
sudo systemctl enable zeroclaw-dingtalk
sudo systemctl start zeroclaw-dingtalk

# 查看状态
sudo systemctl status zeroclaw-dingtalk

# 查看日志
sudo journalctl -u zeroclaw-dingtalk -f
```

## 🔐 安全建议

1. **不要将凭证提交到 Git**
   ```bash
   # 添加到 .gitignore
   echo "~/.zeroclaw/config.toml" >> .gitignore
   ```

2. **使用环境变量**
   ```bash
   export DINGTALK_CLIENT_ID="your-id"
   export DINGTALK_CLIENT_SECRET="your-secret"
   ```

3. **定期轮换凭证**
   - 在钉钉开放平台定期重新生成 Client Secret

4. **使用白名单**
   - 生产环境建议配置 `allowed_users` 限制访问

## 🆚 对比企业微信

| 特性 | 钉钉 Stream | 企业微信 |
|------|------------|----------|
| 本地开发 | ✅ 完美支持 | ❌ 需要备案域名 |
| 配置难度 | ⭐⭐ 简单 | ⭐⭐⭐⭐⭐ 复杂 |
| 连接方式 | WebSocket | HTTP 回调 |
| 是否需要公网 | ❌ 不需要 | ✅ 必须 |
| 消息加密 | 自动 TLS | 需要 AES 解密 |

## 💡 下一步

- 查看 [AGENTS.md](../AGENTS.md) 自定义 AI 助手行为
- 查看 [TOOLS.md](../TOOLS.md) 了解可用工具
- 加入社区: [GitHub Discussions](https://github.com/theonlyhennygod/zeroclaw/discussions)

---

## 📝 配置模板

### 最小配置

```toml
api_key = "your-glm-api-key"
default_provider = "glm"
default_model = "glm-5"

[channels_config.dingtalk]
client_id = "dingxxxxxx"
client_secret = "your-secret"
allowed_users = ["*"]
```

### 完整配置示例

```toml
api_key = "your-api-key"
default_provider = "glm"
default_model = "glm-5"
default_temperature = 0.7

[channels_config]
cli = true

[channels_config.dingtalk]
client_id = "dingxxxxxxxxxxxxxxxxx"
client_secret = "your-client-secret-from-dingtalk"
allowed_users = ["user1", "user2", "user3"]  # 或 ["*"] 允许所有人

[memory]
backend = "sqlite"
auto_save = true

[autonomy]
level = "supervised"
workspace_only = true

[runtime]
kind = "native"
```

---

遇到问题？欢迎提交 [Issue](https://github.com/theonlyhennygod/zeroclaw/issues)！
