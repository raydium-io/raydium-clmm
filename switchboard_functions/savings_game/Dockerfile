# syntax=docker/dockerfile:1.4
FROM switchboardlabs/sgx-function AS builder

# install protoc compiler
# RUN --mount=type=cache,id=apt-cache,target=/var/cache/apt,sharing=locked \
#     --mount=type=cache,id=apt-lib,target=/var/lib/apt,sharing=locked \
#     --mount=type=cache,id=debconf,target=/var/cache/debconf,sharing=locked \
#     set -exu && \
#     DEBIAN_FRONTEND=noninteractive apt update && \
#     apt -y --no-install-recommends install \
#     protobuf-compiler 


# First copy the anchor program so we can re-use the type definitions
WORKDIR /home/root/basic-oracle-example
COPY ./Cargo.lock ./Cargo.lock
WORKDIR /home/root/basic-oracle-example/programs/02_usdy_usdc_oracle
COPY ./programs /home/root/basic-oracle-example/programs

WORKDIR /home/root/basic-oracle-example/switchboard-functions/savings_game
COPY ./switchboard-functions/savings_game/Cargo.lock \
     ./switchboard-functions/savings_game/Cargo.toml \
     ./
COPY ./switchboard-functions/savings_game/src/ ./src/

RUN --mount=type=cache,target=/usr/local/cargo/registry,id=${TARGETPLATFORM} \
    --mount=type=cache,target=target,id=${TARGETPLATFORM} \
    cargo build --release && \
    cargo strip && \
    mv target/release/balancer-oracle-function /sgx/app

FROM switchboardlabs/sgx-function

# Copy the binary
WORKDIR /sgx
COPY --from=builder /sgx/app /sgx
COPY ./switchboard-functions/savings_game/config/app.manifest.template /sgx/app.manifest.template

# Get the measurement from the enclave
RUN rm -f /measurement.txt && \
    /get_measurement.sh && \
    cat /measurement.txt

ENTRYPOINT ["bash", "/boot.sh"]