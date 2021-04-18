FROM debian:stable-slim

RUN apt-get update && \
    apt-get install -y cmake dosfstools g++ python3 rsync uuid uuid-dev && \
    rm -rf /var/lib/apt/lists/*

ADD main.sh /

ENTRYPOINT /main.sh
