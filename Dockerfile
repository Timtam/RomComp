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

FROM alpine:3.20 AS maxcso_builder

WORKDIR /maxcso

RUN apk update && \
    apk add --no-cache build-base gcc git libuv-dev lz4-dev pkgconf zlib-dev && \
    git clone https://github.com/unknownbrackets/maxcso && \
    cd maxcso && \
    git checkout tags/v1.13.0 && \
    make

FROM alpine:3.20

RUN echo "@testing https://dl-cdn.alpinelinux.org/alpine/edge/testing" >> /etc/apk/repositories && \
   apk update && \
    apk add --no-cache dolphin-emu libuv mame-tools@testing

COPY --from=maxcso_builder /maxcso/maxcso/maxcso /usr/bin/
COPY --from=romcomp_builder /app/romcomp/romcomp /usr/bin/

WORKDIR /roms

VOLUME ["/roms"]

ENTRYPOINT ["romcomp"]
