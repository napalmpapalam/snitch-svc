package config

import (
	"gitlab.com/distributed_lab/kit/comfig"
	"gitlab.com/distributed_lab/kit/kv"
)

type Config interface {
	comfig.Logger
	Discordrer
	Telegramrer
}

type config struct {
	comfig.Logger
	Discordrer
	Telegramrer
	getter kv.Getter
}

func New(getter kv.Getter) Config {
	logger := comfig.NewLogger(getter, comfig.LoggerOpts{})
	return &config{
		Logger:      logger,
		getter:      getter,
		Discordrer:  NewDiscorder(getter),
		Telegramrer: NewTelegramer(getter),
	}
}
