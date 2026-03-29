FROM alpine:3.20

RUN apk add --no-cache ca-certificates tini

COPY docker-build/mhost /usr/local/bin/mhost
COPY docker-build/mhostd /usr/local/bin/mhostd

RUN chmod +x /usr/local/bin/mhost /usr/local/bin/mhostd && \
    mkdir -p /root/.mhost

EXPOSE 9090 80 443

ENTRYPOINT ["/sbin/tini", "--"]
CMD ["mhostd"]
