FROM rust:slim AS planner
    WORKDIR dependencies
    RUN cargo install cargo-chef 
    COPY . .
    RUN cargo chef prepare  --recipe-path recipe.json


# Only rebuild dependencies if recipe.json changes
FROM rust as cacher
    WORKDIR dependencies
    RUN cargo install cargo-chef
    COPY --from=planner /dependencies/recipe.json recipe.json
    RUN cargo chef cook --recipe-path recipe.json


FROM rust as tester
    RUN apt-get update && \
        apt-get install -y cmake dosfstools g++ python3 rsync uuid uuid-dev && \
        rm -rf /var/lib/apt/lists/*

    RUN mkdir -p /dependencies/target
    COPY --from=cacher /dependencies/target /dependencies/target
    COPY --from=cacher /usr/local/cargo /usr/local/cargo

    ARG docker_dir
    ADD "$docker_dir/main.sh" /
    RUN chmod +x /main.sh
    ENTRYPOINT /main.sh
