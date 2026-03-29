#!/bin/sh
# Simulates a periodic cleanup task
# This script runs continuously — mhost's cron_restart config will restart it on schedule

LOG_DIR="${LOG_DIR:-/tmp/mhost-cron-logs}"
mkdir -p "$LOG_DIR"

echo "{\"level\":\"info\",\"message\":\"Cleanup worker started\",\"pid\":$$,\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}"

count=0
while true; do
    count=$((count + 1))

    # Simulate cleanup work
    files_cleaned=$((RANDOM % 20))
    bytes_freed=$((RANDOM % 1024 * 1024))

    echo "{\"level\":\"info\",\"message\":\"Cleanup cycle #${count}\",\"files_cleaned\":${files_cleaned},\"bytes_freed\":${bytes_freed},\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}"

    # Write a report file
    echo "Cleanup #${count} at $(date): cleaned ${files_cleaned} files, freed ${bytes_freed} bytes" >> "$LOG_DIR/cleanup-report.txt"

    sleep 30
done
