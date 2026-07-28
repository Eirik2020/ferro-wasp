# syntax=docker/dockerfile:1

# Keep the Python patch release and multi-platform image digest explicit so
# contributors use the same analysis runtime and base filesystem. Rust,
# mdBook, and Python packages are pinned independently below.
FROM python:3.13.14-slim-bookworm@sha256:9d7f287598e1a5a978c015ee176d8216435aaf335ed69ac3c38dd1bbb10e8d64

ARG DEBIAN_FRONTEND=noninteractive
ARG DEV_USER=ferrowasp
ARG DEV_UID=1000
ARG DEV_GID=1000
ARG RUSTUP_VERSION=1.29.0
ARG CODEX_VERSION=0.143.0
ARG CODEX_INSTALLER_SHA256=0e477619f3d4a2ae4b706ffa39b9aa82fef0dac86e999bf605cbbaf55bf6504b

ENV CARGO_HOME=/home/${DEV_USER}/.cargo \
    RUSTUP_HOME=/home/${DEV_USER}/.rustup \
    CARGO_TARGET_DIR=/home/${DEV_USER}/.cache/ferrowasp-target \
    PATH=/home/${DEV_USER}/.cargo/bin:${PATH} \
    MPLBACKEND=Agg \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        bash-completion \
        bubblewrap \
        build-essential \
        ca-certificates \
        curl \
        file \
        git \
        less \
        libcap2-bin \
        libudev-dev \
        libusb-1.0-0-dev \
        openssh-client \
        pkg-config \
        procps \
        ripgrep \
        strace \
        unzip \
        util-linux \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto "=https" --tlsv1.2 --fail --location --silent --show-error \
        "https://raw.githubusercontent.com/openai/codex/rust-v${CODEX_VERSION}/scripts/install/install.sh" \
        --output /tmp/install-codex.sh \
    && printf "%s  %s\n" "${CODEX_INSTALLER_SHA256}" /tmp/install-codex.sh \
        | sha256sum --check --strict \
    && CODEX_HOME=/opt/codex \
        CODEX_INSTALL_DIR=/usr/local/bin \
        CODEX_NON_INTERACTIVE=1 \
        CODEX_RELEASE="${CODEX_VERSION}" \
        sh /tmp/install-codex.sh \
    && rm /tmp/install-codex.sh \
    && test "$(codex --version)" = "codex-cli ${CODEX_VERSION}"

COPY .devcontainer/requirements.lock /tmp/requirements-dev.lock

RUN python -m pip install --no-cache-dir --requirement /tmp/requirements-dev.lock \
    && rm /tmp/requirements-dev.lock \
    && groupadd --gid "${DEV_GID}" "${DEV_USER}" \
    && useradd --create-home --uid "${DEV_UID}" --gid "${DEV_GID}" \
        --shell /bin/bash "${DEV_USER}" \
    && mkdir -p "${CARGO_TARGET_DIR}" \
    && chown -R "${DEV_UID}:${DEV_GID}" \
        "/home/${DEV_USER}" "${CARGO_TARGET_DIR}"

USER ${DEV_USER}

RUN curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" \
        --output /tmp/rustup-init \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal \
        --default-toolchain none \
    && rm /tmp/rustup-init \
    && rustup toolchain install nightly-2026-07-13 \
        --component clippy,rustfmt \
        --target thumbv7em-none-eabihf \
        --profile minimal \
        --no-self-update \
    && rustup toolchain install 1.93.1 \
        --component clippy,rustfmt \
        --profile minimal \
        --no-self-update \
    && rustup toolchain install nightly-2025-12-13 \
        --component rust-analyzer,rustfmt \
        --target thumbv7em-none-eabihf \
        --profile minimal \
        --no-self-update \
    && rustup default nightly-2026-07-13

RUN cargo +1.93.1 install mdbook --version 0.5.2 --locked \
    && cargo +1.93.1 install mdbook-mermaid --version 0.17.0 --locked

RUN rm -rf "${CARGO_TARGET_DIR}" "${CARGO_HOME}/registry" "${CARGO_HOME}/git" \
    && mkdir -p "${CARGO_TARGET_DIR}" "${CARGO_HOME}/registry" "${CARGO_HOME}/git"

WORKDIR /workspace/ferro-wasp

CMD ["bash"]
