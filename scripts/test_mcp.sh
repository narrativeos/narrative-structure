#!/bin/bash
# MCP Server Functional Test Script

PROJECT_DIR="$HOME/.narrativeos/narrative-structure/Projects/0056-08-11_2"
MCP_BIN="/Users/futuremeng/github/narrativeos/narrative-structure/src-tauri/target/debug/narrative-structure-mcp"

if [ ! -f "$MCP_BIN" ]; then
    echo "ERROR: narrative-structure-mcp binary not found at $MCP_BIN"
    echo "Please build first: cd src-tauri && cargo build --bin narrative-structure-mcp"
    exit 1
fi

if [ ! -d "$PROJECT_DIR" ]; then
    echo "ERROR: Project directory not found at $PROJECT_DIR"
    exit 1
fi

PASS=0
FAIL=0

run_test() {
    local name="$1"
    local method="$2"
    local params="$3"
    
    local result
    result=$(echo "$params" | "$MCP_BIN" serve -p "$PROJECT_DIR" 2>/dev/null)
    
    local status
    status=$(echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    if 'error' in d:
        print('FAIL:' + str(d['error'].get('message', 'unknown')))
    elif 'result' in d:
        print('PASS')
    else:
        print('FAIL:no result key')
except Exception as e:
    print('FAIL:' + str(e))
" 2>/dev/null)
    
    if [[ "$status" == "PASS" ]]; then
        echo "✅ $name"
        PASS=$((PASS + 1))
    else
        echo "❌ $name - ${status#FAIL:}"
        FAIL=$((FAIL + 1))
    fi
}

echo "======================================"
echo "NarrativeStructure MCP Server 功能测试"
echo "======================================"
echo ""
echo "Project: $PROJECT_DIR"
echo "Binary:  $MCP_BIN"
echo ""

run_test "Initialize" \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'

run_test "Tools List" \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'

run_test "Get Project Info" \
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_project_info","arguments":{}}}' \
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_project_info","arguments":{}}}'

run_test "Get Page Stats" \
    '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_page_stats","arguments":{}}}' \
    '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_page_stats","arguments":{}}}'

run_test "Get Blocks" \
    '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_blocks","arguments":{"limit":2,"offset":0}}}' \
    '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_blocks","arguments":{"limit":2,"offset":0}}}'

# Skip "Get Block by ID" - requires a real block ID from the database
# run_test "Get Block by ID" \
#     '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_block","arguments":{"id":"<real_block_id>"}}}' \
#     '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_block","arguments":{"id":"<real_block_id>"}}}'

run_test "Get Blocks by Page" \
    '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"get_blocks_by_page","arguments":{"page_start":1,"page_end":1}}}' \
    '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"get_blocks_by_page","arguments":{"page_start":1,"page_end":1}}}'

run_test "Search Blocks" \
    '{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"search_blocks","arguments":{"query":"*","limit":2}}}' \
    '{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"search_blocks","arguments":{"query":"*","limit":2}}}'

run_test "List Assets" \
    '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"list_assets","arguments":{}}}' \
    '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"list_assets","arguments":{}}}'

run_test "Get TOC" \
    '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"get_toc","arguments":{}}}' \
    '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"get_toc","arguments":{}}}'

run_test "Find Asset" \
    '{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"find_asset","arguments":{"pattern":"pdf"}}}' \
    '{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"find_asset","arguments":{"pattern":"pdf"}}}'

run_test "Close Project" \
    '{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"close_project","arguments":{}}}' \
    '{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"close_project","arguments":{}}}'

echo ""
echo "======================================"
echo "Results: $PASS passed, $FAIL failed"
echo "======================================"

if [ $FAIL -gt 0 ]; then
    exit 1
fi