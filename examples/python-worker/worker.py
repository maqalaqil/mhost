import time
import signal
import sys
import json
from datetime import datetime

running = True
tasks_processed = 0

def shutdown(signum, frame):
    global running
    print(json.dumps({"level": "info", "message": "Worker shutting down", "tasks_processed": tasks_processed}))
    running = False

signal.signal(signal.SIGTERM, shutdown)

print(json.dumps({
    "level": "info",
    "message": "Python worker started",
    "pid": __import__('os').getpid(),
    "timestamp": datetime.utcnow().isoformat(),
}))

while running:
    tasks_processed += 1
    print(json.dumps({
        "level": "debug",
        "message": f"Processing task #{tasks_processed}",
        "task_id": tasks_processed,
        "timestamp": datetime.utcnow().isoformat(),
    }))
    time.sleep(5)

print(json.dumps({"level": "info", "message": f"Worker stopped after {tasks_processed} tasks"}))
