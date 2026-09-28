# codehalter bootstrap assisted
FROM alpine:3.24

RUN apk update && apk add --no-cache \
        ca-certificates curl git sudo openssh-client gcompat \
        bash shadow \
        firefox-esr

# rust/GTK rewrite toolchain (rust/), plus virtual display, fonts and icons so
# GUI tests run and screenshots render text and icons inside this container.
RUN apk add --no-cache \
        rust cargo rust-clippy gcc musl-dev \
        gtk4.0-dev libadwaita-dev \
        just \
        xvfb xvfb-run \
        font-dejavu font-noto font-noto-emoji \
        adwaita-icon-theme hicolor-icon-theme librsvg \
        dconf gsettings-desktop-schemas

RUN groupadd --gid 1000 dev \
    && useradd --uid 1000 --gid 1000 -m -s /bin/bash -G wheel dev \
    && echo "dev ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/dev \
    && install -d -o 1000 -g 1000 /home/dev/.config

USER dev
WORKDIR /home/dev
# install.sh installs to ~/.local/bin as the non-root dev user; put it on PATH.
ENV PATH="/home/dev/.local/bin:${PATH}"
# ADD re-fetches the tags JSON every build (via ETag): a new codehalter release
# changes it and busts the cache below; otherwise the install layer stays cached.
ADD https://api.github.com/repos/tbocek/codehalter/tags /tmp/.codehalter-tags.json
RUN curl -sL https://raw.githubusercontent.com/tbocek/codehalter/main/install.sh | bash
