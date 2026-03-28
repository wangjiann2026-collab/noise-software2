#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# deploy.sh — one-click deployment for the noise-api service
# Usage: ./deploy.sh
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_SECRET="change-me-in-production-use-openssl-rand-base64-32"
HEALTH_URL="http://localhost:${API_PORT:-8080}/health"
MAX_RETRIES=30
RETRY_SLEEP=2

# ---- Colour helpers --------------------------------------------------------
red()    { printf '\033[0;31m%s\033[0m\n' "$*"; }
yellow() { printf '\033[0;33m%s\033[0m\n' "$*"; }
green()  { printf '\033[0;32m%s\033[0m\n' "$*"; }
bold()   { printf '\033[1m%s\033[0m\n'   "$*"; }

# ---- Step 1: verify Docker is available ------------------------------------
if ! command -v docker &>/dev/null; then
    red "ERROR: 'docker' not found in PATH. Install Docker and try again."
    red "       https://docs.docker.com/engine/install/"
    exit 1
fi

# Detect docker compose (v2 plugin) or docker-compose (v1 standalone)
if docker compose version &>/dev/null 2>&1; then
    COMPOSE_CMD="docker compose"
elif command -v docker-compose &>/dev/null; then
    COMPOSE_CMD="docker-compose"
else
    red "ERROR: Neither 'docker compose' (v2) nor 'docker-compose' (v1) found."
    red "       Install the Docker Compose plugin: https://docs.docker.com/compose/install/"
    exit 1
fi

bold "==> Using compose command: ${COMPOSE_CMD}"

# ---- Step 2: bootstrap .env from .env.example if absent -------------------
cd "${SCRIPT_DIR}"

if [[ ! -f .env ]]; then
    if [[ ! -f .env.example ]]; then
        red "ERROR: .env.example not found in ${SCRIPT_DIR}. Cannot create .env."
        exit 1
    fi
    cp .env.example .env
    yellow "INFO: .env not found — copied from .env.example."
    yellow "      Review ${SCRIPT_DIR}/.env and set NOISE_JWT_SECRET before going to production."
else
    bold "==> Found existing .env — skipping copy."
fi

# ---- Step 3: warn about default JWT secret ---------------------------------
# Source only the NOISE_JWT_SECRET line to avoid polluting the environment
CURRENT_SECRET="$(grep -E '^NOISE_JWT_SECRET=' .env | cut -d= -f2- | tr -d '[:space:]' || true)"

if [[ -z "${CURRENT_SECRET}" ]]; then
    yellow "WARN: NOISE_JWT_SECRET is not set in .env. Using docker-compose default."
    yellow "      This is insecure. Generate a strong secret with:"
    yellow "        openssl rand -base64 32"
elif [[ "${CURRENT_SECRET}" == "${DEFAULT_SECRET}" ]]; then
    yellow "WARN: NOISE_JWT_SECRET is still the default placeholder value."
    yellow "      Do NOT use this in production. Generate a strong secret with:"
    yellow "        openssl rand -base64 32"
    yellow "      Then update NOISE_JWT_SECRET in ${SCRIPT_DIR}/.env"
fi

# ---- Step 4: build and start containers ------------------------------------
bold "==> Building image and starting containers…"
${COMPOSE_CMD} up -d --build

# ---- Step 5: wait for the health endpoint ----------------------------------
bold "==> Waiting for the API to become healthy (up to $((MAX_RETRIES * RETRY_SLEEP))s)…"

attempt=0
healthy=false

while [[ ${attempt} -lt ${MAX_RETRIES} ]]; do
    attempt=$(( attempt + 1 ))

    if curl --silent --fail --max-time 3 "${HEALTH_URL}" &>/dev/null; then
        healthy=true
        break
    fi

    printf "    Attempt %d/%d — not ready yet, retrying in %ds…\n" \
        "${attempt}" "${MAX_RETRIES}" "${RETRY_SLEEP}"
    sleep "${RETRY_SLEEP}"
done

if [[ "${healthy}" != "true" ]]; then
    red ""
    red "ERROR: API did not become healthy within the timeout."
    red "       Check container logs with:"
    red "         ${COMPOSE_CMD} logs noise-api"
    exit 1
fi

# ---- Step 6: print success message -----------------------------------------
API_PORT_EFFECTIVE="${API_PORT:-8080}"

green ""
green "  Deployment successful!"
green ""
green "  API URL : http://localhost:${API_PORT_EFFECTIVE}"
green "  Health  : ${HEALTH_URL}"
green ""
bold  "  Useful commands:"
printf "    View logs   : %s logs -f noise-api\n" "${COMPOSE_CMD}"
printf "    Stop service: %s down\n" "${COMPOSE_CMD}"
printf "    Restart     : %s restart noise-api\n" "${COMPOSE_CMD}"
green ""
