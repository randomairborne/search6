FROM alpine
ARG TARGETARCH
COPY /${TARGETARCH}-executables/meesearch /usr/bin/

ENTRYPOINT "/usr/bin/meesearch"