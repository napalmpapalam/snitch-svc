FROM golang:1.24.1-alpine as buildbase

WORKDIR /go/src/github.com/napalmpapalam/snitch-svc
COPY vendor .
COPY . .

ENV GO111MODULE="on"
ENV CGO_ENABLED=1
ENV GOOS="linux"

RUN apk add build-base
RUN go build -o /usr/local/bin/snitch-svc github.com/napalmpapalam/snitch-svc

###

FROM alpine:3.9

COPY --from=buildbase /usr/local/bin/snitch-svc /usr/local/bin/snitch-svc
RUN apk add --no-cache ca-certificates

ENTRYPOINT ["snitch-svc"]
