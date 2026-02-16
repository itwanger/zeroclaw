#!/bin/bash
# 企业微信接入快速启动脚本

echo "🦀 ZeroClaw 企业微信接入"
echo ""
echo "配置信息:"
echo "  ✅ 企业ID: ww17c37afdfc31a3e3"
echo "  ✅ 回调URL: https://7550-42-224-155-106.ngrok-free.app/wecom/callback"
echo "  ✅ Token: 79zu"
echo ""

# 检查健康状态
echo "🩺 检查企业微信通道健康状态..."
cargo run --release -- channel doctor
echo ""

echo "📝 接下来的步骤:"
echo ""
echo "1️⃣  在企业微信管理后台配置回调URL:"
echo "   地址: https://7550-42-224-155-106.ngrok-free.app/wecom/callback"
echo "   Token: 79zu"
echo "   EncodingAESKey: SVcmCFHUHtjAqfMiLMPgfwqjCEm0jFTN2ap4d3pRUqA"
echo ""
echo "2️⃣  点击保存后，企业微信会发送验证请求"
echo ""
echo "3️⃣  验证成功后，启动 gateway:"
echo "   cd /Users/itwanger/Documents/GitHub/zeroclaw"
echo "   cargo run --release -- gateway"
echo ""
echo "4️⃣  在企业微信中给机器人发消息测试！"
echo ""
