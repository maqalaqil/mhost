#!/bin/sh
# Simulates an unstable process that crashes after a random time
echo "{\"level\":\"info\",\"message\":\"Crasher started\",\"pid\":$$}"

# Run for 3-8 seconds then crash
RUNTIME=$((3 + RANDOM % 6))
echo "{\"level\":\"warn\",\"message\":\"This process will crash in ${RUNTIME}s\",\"pid\":$$}"
sleep "$RUNTIME"

echo "{\"level\":\"error\",\"message\":\"FATAL: Simulated crash!\",\"exit_code\":1}"
exit 1
