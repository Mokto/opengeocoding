FROM golang:1.21.0-bookworm AS build

WORKDIR /libpostal
RUN git clone --depth 1 https://github.com/openvenues/libpostal /libpostal && \
    apt-get update && apt-get install -y  curl autoconf automake libtool pkg-config && \
    ./bootstrap.sh && \
    ./configure --datadir=/tmp/libpostal_data && \
    make -j4 && \
    make install

WORKDIR /src
# COPY --from=libpostal /usr/local/lib/libpostal* /usr/local/lib
ADD go.mod go.sum ./
RUN go mod download
ADD pkg ./pkg
ADD main.go ./
RUN go build -o main


# FROM golang:1.21.0-bookworm as libpostal

# RUN 




FROM golang:1.21.0-bookworm

ENV LD_LIBRARY_PATH=/usr/local/lib

WORKDIR /libpostal
RUN git clone --depth 1 https://github.com/openvenues/libpostal /libpostal && \
    apt-get update && apt-get install -y  curl autoconf automake libtool pkg-config && \
    ./bootstrap.sh && \
    ./configure --datadir=/tmp/libpostal_data && \
    make -j4 && \
    make install
    
    #  && \
    # rm -rf /tmp/libpostal_data

WORKDIR /app
# COPY --from=build /usr/local/lib/libpostal* /usr/local/lib
# COPY --from=build /usr/local/lib/libpostal.so.1 /usr/local/lib
COPY --from=build /src/main /app/

CMD /app/main
#0 0.092 libpostal.a
#0 0.092 libpostal.la
#0 0.092 libpostal.so
#0 0.092 libpostal.so.1
#0 0.092 libpostal.so.1.0.1
