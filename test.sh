#!/bin/bash
# test-network.sh - Start a test network of Hyrule nodes

echo "🌐 Starting Hyrule Test Network"
echo "================================"
echo

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Kill any existing nodes
pkill -f "hyrule-node start" 2>/dev/null

# Clean up old storage
echo "Cleaning old storage..."
rm -rf node1-storage node2-storage node3-storage

# Build the node
echo "Building Hyrule Node..."

cargo build --release 2>&1 | grep -E "(Finished|error)"
cd ..

if [ $? -ne 0 ]; then
    echo "❌ Build failed"
    exit 1
fi

echo
echo "${GREEN}✓ Build complete${NC}"
echo

# Start Node 1 (Anchor)
echo "${BLUE}Starting Node 1 (Anchor) on port 8080...${NC}"
./hyrule-node/target/release/hyrule-node start \
    --port 8080 \
    --storage-path node1-storage \
    --capacity 5 \
    --anchor \
    > node1.log 2>&1 &
NODE1_PID=$!
echo "  PID: $NODE1_PID"

sleep 2

# Start Node 2 (P2P)
echo "${BLUE}Starting Node 2 (P2P) on port 8081...${NC}"
./hyrule-node/target/release/hyrule-node start \
    --port 8081 \
    --storage-path node2-storage \
    --capacity 5 \
    > node2.log 2>&1 &
NODE2_PID=$!
echo "  PID: $NODE2_PID"

sleep 2

# Start Node 3 (P2P)
echo "${BLUE}Starting Node 3 (P2P) on port 8082...${NC}"
./hyrule-node/target/release/hyrule-node start \
    --port 8082 \
    --storage-path node3-storage \
    --capacity 5 \
    > node3.log 2>&1 &
NODE3_PID=$!
echo "  PID: $NODE3_PID"

sleep 3

echo
echo "${GREEN}✓ Network started successfully!${NC}"
echo
echo "═══════════════════════════════════════════════════════"
echo "${YELLOW}Node Endpoints:${NC}"
echo "  Node 1 (Anchor): http://localhost:8080/status"
echo "  Node 2 (P2P):    http://localhost:8081/status"
echo "  Node 3 (P2P):    http://localhost:8082/status"
echo
echo "${YELLOW}Logs:${NC}"
echo "  Node 1: tail -f node1.log"
echo "  Node 2: tail -f node2.log"
echo "  Node 3: tail -f node3.log"
echo
echo "${YELLOW}Quick Test:${NC}"
echo "  curl http://localhost:8080/status | jq"
echo "  curl http://localhost:8081/repos"
echo
echo "${YELLOW}Stop Network:${NC}"
echo "  pkill -f 'hyrule-node start'"
echo "  # Or press Ctrl+C"
echo "═══════════════════════════════════════════════════════"
echo

# Function to cleanup on exit
cleanup() {
    echo
    echo "${YELLOW}Stopping nodes...${NC}"
    kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null
    echo "${GREEN}✓ Network stopped${NC}"
    exit 0
}

# Trap Ctrl+C
trap cleanup INT TERM

# Wait for all background jobs
wait
