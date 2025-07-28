package config

import (
	"gitlab.com/distributed_lab/figure"
	"gitlab.com/distributed_lab/kit/comfig"
	"gitlab.com/distributed_lab/kit/kv"
	"gitlab.com/distributed_lab/logan/v3/errors"
)

type Discordrer interface {
	Discord() *DiscordConfig
}

type discorder struct {
	getter kv.Getter
	once   comfig.Once
}

type DiscordConfig struct {
	Token           string   `fig:"token"`
	TrackedChannels []string `fig:"tracked_channels"`
	TargetGuildID   string   `fig:"target_guild_id"`
}

func NewDiscorder(getter kv.Getter) Discordrer {
	return &discorder{
		getter: getter,
	}
}

func (e *discorder) Discord() *DiscordConfig {
	return e.once.Do(func() interface{} {
		var cfg DiscordConfig

		err := figure.
			Out(&cfg).
			From(kv.MustGetStringMap(e.getter, "discord")).
			Please()
		if err != nil {
			panic(errors.Wrap(err, "failed to parse saver config"))
		}

		return &cfg
	}).(*DiscordConfig)
}
