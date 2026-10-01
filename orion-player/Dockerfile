FROM node:22-bookworm-slim

ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    ffmpeg \
    file \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libssl-dev \
    libwebkit2gtk-4.1-dev \
    libxdo-dev \
    pkg-config \
    wget \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal

RUN apt-get update && apt-get install -y --no-install-recommends \
    fzf \
    ncurses-bin \
    yt-dlp \
    && install -d /usr/share/licenses/ani-cli \
    && curl -fsSL https://raw.githubusercontent.com/pystardust/ani-cli/v5.1/LICENSE \
        -o /usr/share/licenses/ani-cli/LICENSE \
    && curl -fsSL https://raw.githubusercontent.com/pystardust/ani-cli/v5.1/ani-cli \
        -o /usr/local/bin/ani-cli \
    && chmod 0755 /usr/local/bin/ani-cli \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY package*.json ./
RUN npm ci

COPY . .

EXPOSE 1420

CMD ["npm", "run", "dev"]