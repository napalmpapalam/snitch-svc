FROM golang:1.24.1-alpine as buildbase

WORKDIR /go/src/github.com/napalmpapalam/snitch-svc
RUN apk add build-base
COPY . .
RUN go mod tidy
RUN go mod vendor

ENV GO111MODULE="on"
ENV CGO_ENABLED=1
ENV GOOS="linux"

RUN go build -o /usr/local/bin/snitch-svc github.com/napalmpapalam/snitch-svc

###

FROM alpine:3.9

COPY --from=buildbase /usr/local/bin/snitch-svc /usr/local/bin/snitch-svc
COPY config.yaml .
RUN apk add --no-cache ca-certificates
ENV KV_VIPER_FILE=config.yaml

CMD ["/usr/local/bin/snitch-svc", "run", "all"]
