FROM clux/muslrust:stable AS romcomp_builder

WORKDIR /app

RUN USER=root cargo new --bin romcomp

WORKDIR /app/romcomp

COPY romcomp/Cargo.toml /app/romcomp/Cargo.toml

RUN cargo build --release && \
    rm src/*.rs

COPY romcomp/src ./src

RUN find target/ -type f -name "romcomp*" -path "*-unknown-linux-musl/release/deps/*" -exec rm {} \;
RUN cargo build --release

# copy file to fixed folder

RUN find target/ -type f -name "romcomp" -path "*-unknown-linux-musl/release/*" -exec cp {} . \;

FROM alpine:3.20

RUN echo "@testing https://dl-cdn.alpinelinux.org/alpine/edge/testing" >> /etc/apk/repositories && \
   apk update && \
    apk add --no-cache dolphin-emu mame-tools@testing

COPY --from=romcomp_builder /app/romcomp/romcomp /usr/bin/

WORKDIR /roms

VOLUME ["/roms"]

ENTRYPOINT ["romcomp"]
