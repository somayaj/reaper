#!/usr/bin/env python3
"""Manual jdtls references probe — run from repo root."""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

JDTLS_CANDIDATES = [
    shutil.which("jdtls"),
    "resources/jdtls/bin/jdtls",
    "dist/Reaper.app/Contents/Resources/jdtls/bin/jdtls",
]


def find_jdtls() -> str:
    for c in JDTLS_CANDIDATES:
        if c and os.path.isfile(c):
            return os.path.abspath(c)
    sys.exit("jdtls not found")


def send(proc, obj):
    body = json.dumps(obj)
    msg = f"Content-Length: {len(body)}\r\n\r\n{body}"
    proc.stdin.write(msg.encode())
    proc.stdin.flush()


def read_msg(proc):
    header = b""
    while not header.endswith(b"\r\n\r\n"):
        ch = proc.stdout.read(1)
        if not ch:
            raise EOFError("jdtls stdout closed")
        header += ch
    length = int(
        [l for l in header.decode().split("\r\n") if l.startswith("Content-Length:")][0]
        .split(":", 1)[1]
        .strip()
    )
    body = proc.stdout.read(length)
    if len(body) != length:
        raise EOFError(f"short read {len(body)}/{length}")
    return json.loads(body)


def main():
    jdtls = find_jdtls()
    ws = tempfile.mkdtemp(prefix="reaper-jdtls-refs-")
    try:
        os.makedirs(f"{ws}/src/main/java/com/example", exist_ok=True)
        java_path = f"{ws}/src/main/java/com/example/App.java"
        content = (
            "package com.example;\n"
            "public class App {\n"
            "  public static void greet() {}\n"
            "  public static void main(String[] args) { greet(); }\n"
            "}\n"
        )
        with open(java_path, "w", encoding="utf-8") as f:
            f.write(content)
        with open(f"{ws}/pom.xml", "w", encoding="utf-8") as f:
            f.write(
                '<?xml version="1.0" encoding="UTF-8"?>\n'
                '<project xmlns="http://maven.apache.org/POM/4.0.0">\n'
                "  <modelVersion>4.0.0</modelVersion>\n"
                "  <groupId>com.example</groupId>\n"
                "  <artifactId>app</artifactId>\n"
                "  <version>1.0</version>\n"
                "  <properties>\n"
                "    <maven.compiler.source>17</maven.compiler.source>\n"
                "    <maven.compiler.target>17</maven.compiler.target>\n"
                "  </properties>\n"
                "</project>\n"
            )

        file_uri = f"file://{java_path}"
        proc = subprocess.Popen(
            [jdtls, "-data", f"{ws}/.reaper/jdtls-data"],
            cwd=ws,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )

        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "processId": os.getpid(),
                    "rootUri": f"file://{ws}",
                    "capabilities": {"textDocument": {"references": {}}},
                },
            },
        )
        print("init:", read_msg(proc))
        send(proc, {"jsonrpc": "2.0", "method": "initialized", "params": {}})
        send(
            proc,
            {
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": file_uri,
                        "languageId": "java",
                        "version": 1,
                        "text": content,
                    }
                },
            },
        )

        for i in range(45):
            send(
                proc,
                {
                    "jsonrpc": "2.0",
                    "id": 10 + i,
                    "method": "textDocument/references",
                    "params": {
                        "textDocument": {"uri": file_uri},
                        "position": {"line": 2, "character": 43},
                        "context": {"includeDeclaration": True},
                    },
                },
            )
            while True:
                msg = read_msg(proc)
                if msg.get("id") == 10 + i:
                    result = msg.get("result")
                    print(f"try {i}: count={len(result or [])} sample={result[:1] if result else result}")
                    if result:
                        proc.kill()
                        return
                    break
                elif msg.get("method") == "language/status":
                    print("status:", msg.get("params"))
            time.sleep(1)
        print("no references after retries")
        proc.kill()
    finally:
        shutil.rmtree(ws, ignore_errors=True)


if __name__ == "__main__":
    main()
