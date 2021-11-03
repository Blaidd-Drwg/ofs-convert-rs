FROM rustlang/rust:nightly
    WORKDIR project_root
    ENV CARGO_TARGET_DIR="/target"
    RUN mkdir /target
    RUN apt-get update && \
        apt-get install -y dosfstools python3 rsync sudo uuid uuid-dev && \
        rm -rf /var/lib/apt/lists/*
    RUN mkdir src && touch src/lib.rs
    COPY Cargo.toml .
    COPY Cargo.lock .
    RUN cargo build
    RUN cargo test
    RUN rm -r src

    ARG docker_dir
    ADD "$docker_dir/main.sh" /
    RUN chmod +x /main.sh
    ENTRYPOINT /main.sh
