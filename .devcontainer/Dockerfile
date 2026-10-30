# codehalter bootstrap assisted
FROM archlinux:latest

# Base tools, then the rust/GTK rewrite toolchain (rust/) with node/npm, plus a
# virtual display, fonts and icons so GUI tests run and screenshots render text
# and icons inside this container.
RUN pacman -Syu --noconfirm --needed \
        ca-certificates curl git sudo openssh \
        bash python \
        grep coreutils procps-ng \
        firefox \
        nodejs npm \
        rust gcc pkgconf make \
        gtk4 libadwaita \
        just \
        xorg-server-xvfb xorg-xauth xdotool \
        ttf-dejavu noto-fonts noto-fonts-cjk noto-fonts-emoji \
        adwaita-icon-theme hicolor-icon-theme librsvg \
        ffmpeg \
        gstreamer gst-plugins-base gst-plugins-good gst-libav gst-plugin-gtk4 \
        dconf gsettings-desktop-schemas \
    && pacman -Scc --noconfirm

RUN groupadd --gid 1000 dev \
    && useradd --uid 1000 --gid 1000 -m -s /bin/bash -G wheel dev \
    && echo "dev ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/dev \
    && install -d -o 1000 -g 1000 /home/dev/.config \
    && install -d -o 1000 -g 1000 /home/dev/.config/naivepost \
    && install -d -m 700 -o 1000 -g 1000 /run/user/1000

USER dev
WORKDIR /home/dev
# install.sh installs to ~/.local/bin as the non-root dev user; put it on PATH.
ENV PATH="/home/dev/.local/bin:${PATH}"
# ADD re-fetches the tags JSON every build (via ETag): a new codehalter release
# changes it and busts the cache below; otherwise the install layer stays cached.
ADD https://api.github.com/repos/tbocek/codehalter/tags /tmp/.codehalter-tags.json
RUN curl -sL https://raw.githubusercontent.com/tbocek/codehalter/main/install.sh | bash
