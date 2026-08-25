#!/usr/bin/env python3
"""Minimal debugging SMTP server for local email tests.

Accepts any sender/recipient, captures the message body, and prints it to
stdout. No authentication, no TLS, no real delivery. Used only for proving
the Detectic email endpoint before a real SMTP relay is configured.
"""
import socketserver
import time

HOST = "localhost"
PORT = 2525


class DebugSmtpHandler(socketserver.BaseRequestHandler):
    def handle(self):
        peer = self.client_address
        rfile = self.request.makefile("rb")
        wfile = self.request.makefile("wb")

        def send(text):
            wfile.write((text + "\r\n").encode())
            wfile.flush()

        send("220 testsmtp ESMTP")
        in_data = False
        message = []

        while True:
            line = rfile.readline()
            if not line:
                break
            line = line.decode("utf-8", "replace").rstrip("\r\n")
            if in_data:
                if line == ".":
                    in_data = False
                    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
                    print(f"--- email received at {ts} from {peer} ---")
                    print("\n".join(message))
                    print("--- end ---", flush=True)
                    message = []
                    send("250 OK")
                    continue
                # dot-stuffing unescape
                if line.startswith("."):
                    line = line[1:]
                message.append(line)
                continue

            cmd = line.split(None, 1)
            verb = (cmd[0] if cmd else "").upper()
            arg = cmd[1] if len(cmd) > 1 else ""

            if verb in ("EHLO", "HELO"):
                send(f"250 testsmtp Hello {arg}")
            elif verb == "MAIL":
                send("250 OK")
            elif verb == "RCPT":
                send("250 OK")
            elif verb == "DATA":
                in_data = True
                send("354 End data with <CR><LF>.<CR><LF>")
            elif verb == "QUIT":
                send("221 Bye")
                break
            elif verb == "RSET":
                send("250 OK")
            else:
                send("500 5.5.2 Error: command not recognized")


class ThreadedSMTPServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True


def main():
    print(f"testsmtp starting on {HOST}:{PORT}")
    with ThreadedSMTPServer((HOST, PORT), DebugSmtpHandler) as server:
        server.serve_forever()


if __name__ == "__main__":
    main()
