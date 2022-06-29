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
data = [0] * 1024
data[2] = 1
data[3] = 0
data[4] = 4
tcp_client.send(bytes(data))

# 4.サーバからのレスポンスを受信
response = tcp_client.recv(buffer_size)
print("[*]Received a response : {}".format(response))

