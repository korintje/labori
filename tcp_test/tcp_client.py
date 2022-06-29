# -*- coding : UTF-8 -*-

# 0.ライブラリのインポートと変数定義
import socket

target_ip = "127.0.0.1"
target_port = 50001
buffer_size = 1024

# 1.ソケットオブジェクトの作成
tcp_client = socket.socket(socket.AF_INET, socket.SOCK_STREAM)

# 2.サーバに接続
tcp_client.connect((target_ip,target_port))

# 3.サーバにデータを送信
# data = r'{"Set": { "key": "Interval", "value": "1" }}'
# data = r'{"Get": { "key": "Interval", "value": "0.001" }}'
# data = r'{"Run": {}}'
data = r'{"Stop": {}}'
data_ba = bytes(data, "utf-8")
print(data_ba)
tcp_client.send(data_ba)

# 4.サーバからのレスポンスを受信
response = tcp_client.recv(buffer_size)
print("Received a response : {}".format(response))

