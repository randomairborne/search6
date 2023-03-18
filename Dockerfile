FROM alpine
ARG TARGETARCH
COPY /${TARGETARCH}-executables/search6 /usr/bin/

ENTRYPOINT "/usr/bin/search6"