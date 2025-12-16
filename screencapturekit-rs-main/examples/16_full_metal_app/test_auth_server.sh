#!/bin/bash

echo "Testing Auth0 callback server..."
echo ""
echo "This script will:"
echo "1. Start a test server on port 8080"
echo "2. Send a simulated callback request"
echo "3. Show what the server receives"
echo ""

# Test with curl
echo "Simulating browser callback..."
curl -v "http://127.0.0.1:8080/login/oauth2/code/oidc?code=test_code_123&state=test_state_456"

echo ""
echo "If you see logs about 'Request received' or 'Callback received', the routing is working!"

