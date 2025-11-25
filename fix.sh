#!/bin/bash
# Quick fix for hyrule-node.toml
# Run this to update your config to use the onion address

CONFIG_FILE="/home/Link/.config/hyrule-node/config.toml"

if [ ! -f "$CONFIG_FILE" ]; then
    echo "❌ Config file not found: $CONFIG_FILE"
    exit 1
fi

echo "📝 Backing up current config..."
cp "$CONFIG_FILE" "${CONFIG_FILE}.backup"

echo "🔧 Updating config to use Tor onion service..."

# Update the hyrule_server to onion address
sed -i 's|hyrule_server = "http://localhost:3000"|hyrule_server = "http://hyrule4e3tu7pfdkvvca43senvgvgisi6einpe3d3kpidlk3uyjf7lqd.onion"|g' "$CONFIG_FILE"
sed -i 's|hyrule_server = "http://127.0.0.1:3000"|hyrule_server = "http://hyrule4e3tu7pfdkvvca43senvgvgisi6einpe3d3kpidlk3uyjf7lqd.onion"|g' "$CONFIG_FILE"

# Enable Tor proxy
sed -i 's/enable_proxy = false/enable_proxy = true/g' "$CONFIG_FILE"
sed -i 's/enable_onion_routing = false/enable_onion_routing = true/g' "$CONFIG_FILE"

echo "✅ Config updated!"
echo ""
echo "Changes made:"
echo "  • hyrule_server → onion address"
echo "  • enable_proxy → true"
echo "  • enable_onion_routing → true"
echo ""
echo "Backup saved to: ${CONFIG_FILE}.backup"
echo ""
echo "Verify changes:"
echo "  cat $CONFIG_FILE | grep hyrule_server"
echo "  cat $CONFIG_FILE | grep enable_proxy"
