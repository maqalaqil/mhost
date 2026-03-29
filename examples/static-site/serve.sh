#!/bin/sh
# Simple static file server using Python
PORT=${PORT:-8080}
echo "{\"level\":\"info\",\"message\":\"Static site server starting on port $PORT\",\"pid\":$$}"
cd "$(dirname "$0")"

# Create a simple index.html if it doesn't exist
cat > index.html << 'HTMLEOF'
<!DOCTYPE html>
<html>
<head><title>mhost Example</title></head>
<body>
  <h1>mhost Static Site</h1>
  <p>This site is served by mhost process manager.</p>
</body>
</html>
HTMLEOF

python3 -m http.server "$PORT"
