package config

import (
	"gitlab.com/distributed_lab/figure"
	"gitlab.com/distributed_lab/kit/comfig"
	"gitlab.com/distributed_lab/kit/kv"
	"gitlab.com/distributed_lab/logan/v3/errors"
)

type Telegramrer interface {
	Telegram() *TelegramConfig
}

type telegramrer struct {
	getter kv.Getter
	once   comfig.Once
}

type TelegramConfig struct {
	Token  string `fig:"token"`
	ChatID int64  `fig:"chat_id"`
}

func NewTelegramer(getter kv.Getter) Telegramrer {
	return &telegramrer{
		getter: getter,
	}
}

func (e *telegramrer) Telegram() *TelegramConfig {
	return e.once.Do(func() interface{} {
		var cfg TelegramConfig

		err := figure.
			Out(&cfg).
			From(kv.MustGetStringMap(e.getter, "telegram")).
			Please()
		if err != nil {
			panic(errors.Wrap(err, "failed to parse telegram config"))
		}

		return &cfg
	}).(*TelegramConfig)
}
