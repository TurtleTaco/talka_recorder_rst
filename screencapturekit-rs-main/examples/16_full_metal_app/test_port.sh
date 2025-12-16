#!/bin/bash

echo "Checking if port 8080 is available..."
if lsof -Pi :8080 -sTCP:LISTEN -t >/dev/null 2>&1 ; then
    echo "❌ Port 8080 is already in use by:"
    lsof -Pi :8080 -sTCP:LISTEN
    echo ""
    echo "Kill it with: kill -9 <PID>"
else
    echo "✅ Port 8080 is available"
fi

echo ""
echo "Testing if we can bind to port 8080..."
nc -l 8080 &
NC_PID=$!
sleep 1

if kill -0 $NC_PID 2>/dev/null; then
    echo "✅ Successfully bound to port 8080"
    kill $NC_PID
else
    echo "❌ Failed to bind to port 8080"
fi

