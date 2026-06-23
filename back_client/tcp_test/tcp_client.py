import json
import socket
import time

TARGET = ("127.0.0.1", 50001)


def request(command: dict) -> dict:
    payload = json.dumps(command).encode("utf-8") + b"\n"
    with socket.create_connection(TARGET, timeout=5) as client:
        client.sendall(payload)
        response = client.makefile("rb").readline()
    return json.loads(response)


print(request({"Set": {"key": "Interval", "value": "0.001"}}))
print(request({"Run": {}}))
time.sleep(10)
print(request({"Stop": {}}))
