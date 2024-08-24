FROM clux/muslrust:stable AS romcomp_builder

WORKDIR /libcue

RUN apt-get update && \
    apt-get install -y bison flex git && \
    git clone https://github.com/lipnitsk/libcue.git && \
    cd libcue && \
    git checkout tags/v2.3.0 && \
    mkdir build && cd build && \
    CC="musl-gcc -fPIC -pie" LDFLAGS="-L$PREFIX/lib" CFLAGS="-I$PREFIX/include" cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX:PATH=$PREFIX ../ && \
    make && make install

WORKDIR /app

RUN USER=root cargo new --bin romcomp

WORKDIR /app/romcomp

COPY romcomp/Cargo.toml /app/romcomp/Cargo.toml

RUN cargo build --release && \
    rm src/*.rs

COPY romcomp/src ./src

RUN find target/ -type f -name "romcomp*" -path "*-unknown-linux-musl/release/deps/*" -exec rm {} \; && \
    RUSTFLAGS='-L /musl/lib' cargo build --release && \
    find target/ -type f -name "romcomp" -path "*-unknown-linux-musl/release/*" -exec cp {} . \;

FROM alpine:3.20 AS c_builder

WORKDIR /build

RUN apk update && \
    apk add --no-cache build-base gcc git libuv-dev lz4-dev pkgconf zlib-dev

RUN git clone https://github.com/unknownbrackets/maxcso && \
    cd maxcso && \
    git checkout tags/v1.13.0 && \
    make

FROM golang:1.19-alpine AS go_builder

WORKDIR /rom64

RUN apk update && \
    apk add --no-cache git && \
    git clone https://github.com/mroach/rom64 && \
    cd rom64 && \
    git checkout tags/v0.5.4 && \
    go get -d && \
    go build -ldflags "-s -w" main.go

FROM ghcr.io/graalvm/native-image:java11-21.2 AS java_builder

WORKDIR /build

RUN microdnf install git && \
    git clone https://github.com/XanderXAJ/BitButcher && \
    cd BitButcher && \
    ./make.sh && \
    native-image --static -jar bin/BitButcher.jar

FROM alpine:3.20

RUN echo "@testing https://dl-cdn.alpinelinux.org/alpine/edge/testing" >> /etc/apk/repositories && \
    apk update && \
    apk add --no-cache dolphin-emu gcompat libuv mame-tools@testing

COPY --from=c_builder /build/maxcso/maxcso /usr/bin/
COPY --from=go_builder /rom64/rom64/main /usr/bin/rom64
COPY --from=java_builder /build/BitButcher/BitButcher /usr/bin/
COPY --from=romcomp_builder /app/romcomp/romcomp /usr/bin/

WORKDIR /roms

VOLUME ["/roms"]

ENTRYPOINT ["romcomp"]
