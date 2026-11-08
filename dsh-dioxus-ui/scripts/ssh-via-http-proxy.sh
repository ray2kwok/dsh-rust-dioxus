#!/usr/bin/env python3
"""ssh ProxyCommand：把 SSH 连接经系统 HTTP 代理的 CONNECT 隧道转发。

本机没装 nc / socat / corkscrew，故用 python3 实现（系统自带 3.14）。

用法（由 ssh 调用，%h %p 由 ssh 替换）：
    ssh -o ProxyCommand="/path/to/ssh-via-http-proxy.sh %h %p" git@github.com

环境变量：
    DSH_PROXY_HOST  默认 127.0.0.1
    DSH_PROXY_PORT  默认 7897
    DSH_PROXY_URL   也可直接给 http://host:port（优先）
"""

import os
import re
import socket
import sys
import threading

DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 7897


def proxy_addr() -> tuple[str, int]:
    url = os.environ.get("DSH_PROXY_URL", "").strip()
    if url:
        match = re.match(r"^https?://([^:/]+)(?::(\d+))?/?$", url)
        if match:
            return match.group(1), int(match.group(2) or 80)
    return (
        os.environ.get("DSH_PROXY_HOST", DEFAULT_HOST),
        int(os.environ.get("DSH_PROXY_PORT", DEFAULT_PORT)),
    )


def upstream_to_sock(sock: socket.socket) -> None:
    """stdin → 代理隧道。

    注意：**不要**在 stdin EOF 时 shutdown(SHUT_WR)：多数 HTTP 代理（Clash 等）
    把半关闭当成整条连接关闭，会立刻断掉隧道，ssh 连 banner 都收不到。
    读方向由服务端 EOF 自然结束。
    """
    try:
        while True:
            data = os.read(0, 65536)
            if not data:
                break
            sock.sendall(data)
    except OSError:
        pass


def main() -> int:
    if len(sys.argv) < 3:
        sys.stderr.write("usage: ssh-via-http-proxy.sh <host> <port>\n")
        return 2

    host, port = sys.argv[1], int(sys.argv[2])
    phost, pport = proxy_addr()

    try:
        sock = socket.create_connection((phost, pport), timeout=20)
    except OSError as err:
        sys.stderr.write(f"proxy {phost}:{pport} 连接失败: {err}\n")
        return 1

    request = (
        f"CONNECT {host}:{port} HTTP/1.1\r\n"
        f"Host: {host}:{port}\r\n"
        f"Proxy-Connection: keep-alive\r\n\r\n"
    )
    sock.sendall(request.encode())

    # 只读到响应头结束，之后的数据可能已经粘在缓冲区里，一并转给 ssh
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = sock.recv(4096)
        if not chunk:
            sys.stderr.write("代理在 CONNECT 阶段断开\n")
            return 1
        buf += chunk

    head, rest = buf.split(b"\r\n\r\n", 1)
    status = head.split(b"\r\n", 1)[0].decode(errors="replace")
    if " 200 " not in status:
        sys.stderr.write(f"代理拒绝 CONNECT {host}:{port}: {status}\n")
        return 1

    sock.settimeout(None)
    if rest:
        os.write(1, rest)

    threading.Thread(target=upstream_to_sock, args=(sock,), daemon=True).start()

    try:
        while True:
            data = sock.recv(65536)
            if not data:
                break
            os.write(1, data)
    except OSError:
        pass
    finally:
        try:
            sock.close()
        except OSError:
            pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
