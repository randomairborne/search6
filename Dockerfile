FROM alpine
ARG TARGETARCH
COPY /${TARGETARCH}-executables/minixpd /usr/bin/

ENTRYPOINT "/usr/bin/minixpd"