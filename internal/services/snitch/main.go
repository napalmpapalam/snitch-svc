package snitch

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/bwmarrin/discordgo"
	tgbotapi "github.com/go-telegram-bot-api/telegram-bot-api/v5"
	"github.com/napalmpapalam/snitch-svc/internal/config"
	"gitlab.com/distributed_lab/logan"
	"gitlab.com/distributed_lab/running"
)

// 1257044413302046853

func Run(cfg config.Config, ctx context.Context) {
	discord, _ := discordgo.New("Bot " + cfg.Discord().Token)
	defer discord.Close()

	tgBot, err := tgbotapi.NewBotAPI(cfg.Telegram().Token)
	if err != nil {
		log.Panic(err)
	}

	running.WithBackOff(ctx, cfg.Log(), "snitch", func(ctx context.Context) error {
		discord.AddHandler(func(s *discordgo.Session, vs *discordgo.VoiceStateUpdate) {
			if vs.GuildID != cfg.Discord().TargetGuildID {
				return
			}
			if vs.BeforeUpdate != nil {
				return
			}
			if vs.ChannelID == "" {
				return
			}
			if !contains(cfg.Discord().TrackedChannels, vs.ChannelID) {
				return
			}

			user, _ := s.User(vs.UserID)
			channel, _ := s.Channel(vs.ChannelID)

			cfg.Log().Info("user joined channel", logan.F{
				"user":    user.Username,
				"channel": channel.Name,
			})

			member, _ := s.GuildMember(vs.GuildID, vs.UserID)
			displayName := user.Username
			if member.Nick != "" {
				displayName = fmt.Sprintf("%s (%s)", member.Nick, user.Username)
			}

			msg := fmt.Sprintf("🎤 %s joined voice channel\n📍 Channel: %s", displayName, channel.Name)

			tgMsg := tgbotapi.NewMessage(cfg.Telegram().ChatID, msg)
			_, err := tgBot.Send(tgMsg)
			if err != nil {
				cfg.Log().Error("error sending message to telegram", logan.F{
					"error":   err,
					"msg":     tgMsg,
					"chat_id": cfg.Telegram().ChatID,
				})
			}

			cfg.Log().Info("message sent to telegram", logan.F{
				"message": msg,
			})
		})

		discord.Open()

		<-ctx.Done()
		return ctx.Err()
	}, 5*time.Second, 5*time.Second, 5*time.Second)
}

func contains(slice []string, item string) bool {
	for _, s := range slice {
		if s == item {
			return true
		}
	}
	return false
}
