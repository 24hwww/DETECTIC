# Detectic backend — ingestion API (M6).
#
# Usage:
#   docker build -t detectic-backend ./backend
#   docker run -d -p 8080:8080 \
#     -v detectic-data:/data \
#     -e DETECTIC_SENSORS='{"ex520-001":"<secret>"}' \
#     -e DETECTIC_MASTER_SECRET='<master-secret>' \
#     detectic-backend
#
# The API listens on 0.0.0.0:8080. Terminate TLS with a reverse proxy
# (Caddy/Traefik/nginx) in front of this container for production HTTPS.
FROM python:3.12-slim

WORKDIR /app

# Copy only the backend module (no repo secrets, no router artifacts).
COPY server.py sensors.json.example ./

ENV PYTHONUNBUFFERED=1

# Data volume: SQLite DB lives here.
VOLUME ["/data"]

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD python3 -c "import urllib.request;urllib.request.urlopen('http://127.0.0.1:8080/api/v1/healthz')" || exit 1

CMD ["python3", "-u", "server.py", \
     "--host", "0.0.0.0", "--port", "8080", \
     "--db", "/data/backend.db", \
     "--master-secret", "$DETECTIC_MASTER_SECRET"]
