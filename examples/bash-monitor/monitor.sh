#!/bin/sh
# System health monitor — outputs JSON metrics every 10 seconds
# Demonstrates mhost managing a bash script that produces structured logs

echo "{\"level\":\"info\",\"message\":\"System monitor started\",\"pid\":$$,\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}"

while true; do
    # CPU load
    load=$(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}' || uptime | awk -F'load average:' '{print $2}' | awk '{print $1}' | tr -d ',')

    # Memory (macOS)
    if command -v vm_stat > /dev/null 2>&1; then
        pages_free=$(vm_stat | grep "Pages free" | awk '{print $3}' | tr -d '.')
        pages_active=$(vm_stat | grep "Pages active" | awk '{print $3}' | tr -d '.')
        mem_free_mb=$(( (pages_free * 4096) / 1048576 ))
        mem_active_mb=$(( (pages_active * 4096) / 1048576 ))
    else
        mem_free_mb=$(free -m 2>/dev/null | awk '/Mem:/{print $4}' || echo "0")
        mem_active_mb=$(free -m 2>/dev/null | awk '/Mem:/{print $3}' || echo "0")
    fi

    # Disk usage
    disk_used=$(df -h / | tail -1 | awk '{print $5}' | tr -d '%')

    # Process count
    proc_count=$(ps aux | wc -l | tr -d ' ')

    echo "{\"level\":\"info\",\"message\":\"System metrics\",\"load\":\"${load}\",\"mem_free_mb\":${mem_free_mb:-0},\"mem_active_mb\":${mem_active_mb:-0},\"disk_used_pct\":${disk_used},\"process_count\":${proc_count},\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}"

    # Alert if disk is high
    if [ "$disk_used" -gt 90 ]; then
        echo "{\"level\":\"error\",\"message\":\"DISK USAGE CRITICAL\",\"disk_used_pct\":${disk_used},\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}"
    fi

    sleep 10
done
