package snitch

import (
	"context"
	"fmt"
	"log"
	"math/rand"
	"sync"
	"time"

	"github.com/bwmarrin/discordgo"
	tgbotapi "github.com/go-telegram-bot-api/telegram-bot-api/v5"
	"github.com/napalmpapalam/snitch-svc/internal/config"
	"gitlab.com/distributed_lab/logan"
	"gitlab.com/distributed_lab/running"
)

type voiceStateCache struct {
	mu     sync.RWMutex
	states map[string]*discordgo.VoiceState
}

var cache = &voiceStateCache{
	states: make(map[string]*discordgo.VoiceState),
}

func Run(cfg config.Config, ctx context.Context) {
	discord, _ := discordgo.New("Bot " + cfg.Discord().Token)
	defer discord.Close()

	tgBot, err := tgbotapi.NewBotAPI(cfg.Telegram().Token)
	if err != nil {
		log.Panic(err)
	}

	running.WithBackOff(ctx, cfg.Log(), "snitch", func(ctx context.Context) error {
		discord.AddHandler(func(s *discordgo.Session, vs *discordgo.VoiceStateUpdate) {
			// Get cached state for this user
			cache.mu.RLock()
			cachedState, exists := cache.states[vs.UserID]
			cache.mu.RUnlock()

			// User left a voice channel
			if vs.ChannelID == "" && exists && cachedState.ChannelID != "" {
				// Check if we should track this event (for the channel they left)
				if vs.GuildID == cfg.Discord().TargetGuildID && contains(cfg.Discord().TrackedChannels, cachedState.ChannelID) {
					handleUserLeftChannel(cfg, s, tgBot, vs, cachedState.ChannelID)
				}

				// Remove from cache
				cache.mu.Lock()
				delete(cache.states, vs.UserID)
				cache.mu.Unlock()
				return
			}

			// User joined a voice channel
			if vs.ChannelID != "" {
				// Check if we should track this event
				if shouldTrackVoiceEvent(cfg, vs) {
					// Check if this is a channel switch or a new join
					if exists && cachedState.ChannelID != "" && cachedState.ChannelID != vs.ChannelID {
						// User switched channels - handle as leave then join
						if contains(cfg.Discord().TrackedChannels, cachedState.ChannelID) {
							handleUserLeftChannel(cfg, s, tgBot, vs, cachedState.ChannelID)
						}
						handleUserJoinedChannel(cfg, s, tgBot, vs)
					} else if !exists || cachedState.ChannelID == "" {
						// User joined from outside any voice channel
						handleUserJoinedChannel(cfg, s, tgBot, vs)
					}
				}

				// Update cache
				cache.mu.Lock()
				cache.states[vs.UserID] = &discordgo.VoiceState{
					UserID:    vs.UserID,
					ChannelID: vs.ChannelID,
					GuildID:   vs.GuildID,
				}
				cache.mu.Unlock()
			}
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

func shouldTrackVoiceEvent(cfg config.Config, vs *discordgo.VoiceStateUpdate) bool {
	if vs.GuildID != cfg.Discord().TargetGuildID {
		return false
	}
	return contains(cfg.Discord().TrackedChannels, vs.ChannelID)
}

func formatUserDisplayName(s *discordgo.Session, guildID, userID string) string {
	user, err := s.User(userID)
	if err != nil {
		return "Unknown User"
	}

	emoji := getRandomEmoji(user.Username)

	member, err := s.GuildMember(guildID, userID)
	if err != nil || member.Nick == "" {
		return fmt.Sprintf("%s %s", emoji, user.Username)
	}

	return fmt.Sprintf("%s %s (%s) %s", emoji, member.Nick, user.Username, emoji)
}

func sendTelegramNotification(cfg config.Config, bot *tgbotapi.BotAPI, message string) error {
	msg := tgbotapi.NewMessage(cfg.Telegram().ChatID, message)
	_, err := bot.Send(msg)
	if err != nil {
		cfg.Log().Error("error sending message to telegram", logan.F{
			"error":   err,
			"msg":     msg,
			"chat_id": cfg.Telegram().ChatID,
		})
		return err
	}

	cfg.Log().Info("message sent to telegram", logan.F{
		"message": message,
	})
	return nil
}

func handleUserJoinedChannel(cfg config.Config, s *discordgo.Session, bot *tgbotapi.BotAPI, vs *discordgo.VoiceStateUpdate) {
	user, _ := s.User(vs.UserID)
	channel, _ := s.Channel(vs.ChannelID)

	cfg.Log().Info("user joined channel", logan.F{
		"user":    user.Username,
		"channel": channel.Name,
	})

	displayName := formatUserDisplayName(s, vs.GuildID, vs.UserID)
	msg := fmt.Sprintf("🎤 %s joined voice channel\n📍 Channel: %s", displayName, channel.Name)

	sendTelegramNotification(cfg, bot, msg)
}

func handleUserLeftChannel(cfg config.Config, s *discordgo.Session, bot *tgbotapi.BotAPI, vs *discordgo.VoiceStateUpdate, previousChannelID string) {
	user, _ := s.User(vs.UserID)
	channel, _ := s.Channel(previousChannelID)

	cfg.Log().Info("user left channel", logan.F{
		"user":    user.Username,
		"channel": channel.Name,
	})

	displayName := formatUserDisplayName(s, vs.GuildID, vs.UserID)
	msg := fmt.Sprintf("👋 %s left voice channel\n📍 Channel: %s", displayName, channel.Name)

	sendTelegramNotification(cfg, bot, msg)
}

func getRandomEmoji(username string) string {
	if username == "rilein1" {
		return "💩"
	}

	emojis := []string{
		// Gaming & Entertainment
		"🎮", "🎯", "🎪", "🎨", "🎭", "🎰", "🎲", "🎸", "🎺", "🎻", "🎹", "🥁", "🎧", "🎤", "🎬", "🎨", "🎯", "🎳", "🎱",
		// Animals
		"🦄", "🦋", "🐉", "🦅", "🦉", "🦜", "🦩", "🦚", "🐆", "🦓", "🦒", "🦔", "🦦", "🦥", "🦨", "🦡", "🐿️", "🦫", "🐻", "🐨",
		"🐼", "🦘", "🦙", "🐪", "🦏", "🦛", "🐘", "🦣", "🐊", "🦖", "🦕", "🐙", "🦑", "🦐", "🦞", "🦀", "🐠", "🐟", "🐡", "🦈",
		// Nature & Space
		"🌟", "⭐", "🔥", "💫", "✨", "☄️", "🌠", "🌌", "🌃", "🌆", "🌇", "🌉", "🌊", "🌋", "🗻", "🏔️", "⛰️", "🏕️", "🏖️",
		"🏜️", "🏞️", "🌳", "🌲", "🌴", "🌵", "🌺", "🌻", "🌷", "🌹", "🥀", "🌾", "🌿", "🍀", "🍄", "🌸", "💐", "🌼", "🌱", "🌿",
		// Expressions & Fun (including 💩)
		"💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩",
		"💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩",
		"🥳", "🤖", "😎", "🤠", "🥷", "👻", "👽", "👾", "🤡", "🎃", "😈", "👹", "👺", "🤯", "🤩", "🥸", "🧙", "🧚", "🧛", "💩",
		"💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩",
		"💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩",
		"💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩", "💩",
		"🧜", "🧞", "🧟", "🦸", "🦹", "🗿", "🤺", "🏇", "⛷️", "🏂", "🏌️", "🏄", "🚣", "🏊", "⛹️", "🏋️", "🚴", "🚵", "🤸", "🤹",
		"🧘", "😂", "🤣", "😊", "😇", "🙃", "😉", "😌", "😍", "🥰", "😘", "😗", "😙", "😚", "😋", "😛", "😝", "😜", "🤪", "🤨",
		// Food & Objects
		"🍕", "🍔", "🍟", "🌮", "🌯", "🥙", "🧆", "🍖", "🍗", "🥓", "🧀", "🥨", "🥐", "🍞", "🥖", "🥯", "🧇", "🥞", "🍩", "🍪",
		"🎂", "🍰", "🧁", "🥧", "🍮", "🍭", "🍬", "🍫", "🍿", "🧃", "🧋", "🍇", "🍈", "🍉", "🍊", "🍋", "🍌", "🍍", "🥭", "🍎",
		// Symbols & Misc
		"💎", "💰", "🏆", "🥇", "🥈", "🥉", "🏅", "🎖️", "🏵️", "🎗️", "🎁", "🎈", "🎊", "🎉", "🧨", "🎆", "🎇", "🔮", "🪄", "🎯",
		"🛸", "🚀", "⚡", "💥", "💢", "🦭",
		// More unique ones
		"🌋", "🗾", "🏛️", "🏰", "🏯", "🏟️", "🗽", "🗼", "🏗️", "🏭", "🏠", "🏡", "🏘️", "🏚️", "🏢", "🏬", "🏣", "🏤", "🏥", "🏦",
		"⚓", "⛵", "🚤", "🛳️", "⛴️", "🚢", "✈️", "🛩️", "🛫", "🛬", "🪂", "💺", "🚁", "🚟", "🚠", "🚡", "🛰️", "🛎️", "⏰", "🌡️",
		// More Animals & Insects
		"🐝", "🐞", "🦗", "🕷️", "🕸️", "🦂", "🦟", "🪰", "🪱", "🦠", "🐢", "🐍", "🦎", "🐙", "🦑", "🦐", "🦞", "🦀", "🐚", "🦪",
		"🐌", "🦋", "🐛", "🐜", "🐝", "🪲", "🐞", "🦗", "🪳", "🕷️", "🕸️", "🦂", "🦟", "🪰", "🪱", "🐵", "🐒", "🦍", "🦧", "🐶",
		"🐕", "🦮", "🐩", "🐺", "🦊", "🦝", "🐱", "🐈", "🐈‍⬛", "🦁", "🐯", "🐅", "🐆", "🐴", "🫎", "🫏", "🐎", "🦄", "🦓", "🦌",
		// Technology & Tools
		"💻", "🖥️", "🖨️", "⌨️", "🖱️", "🖲️", "💾", "💿", "📀", "🧮", "📱", "☎️", "📞", "📟", "📠", "📡", "📺", "📻", "🎥", "📹",
		"📷", "📸", "📼", "🔍", "🔎", "🕯️", "💡", "🔦", "🏮", "🪔", "📔", "📕", "📖", "📗", "📘", "📙", "📚", "📓", "📒", "📃",
		// Weather & Nature Elements
		"☀️", "🌤️", "⛅", "🌥️", "☁️", "🌦️", "🌧️", "⛈️", "🌩️", "🌨️", "❄️", "☃️", "⛄", "🌬️", "💨", "💧", "💦", "🫧", "☔", "☂️",
		"🌊", "🌫️", "🌪️", "🌈", "🌀", "🌁", "🌄", "🌅", "🌆", "🌇", "🌉", "🌊", "🏔️", "⛰️", "🌋", "🗻", "🏕️", "🏖️", "🏜️", "🏝️",
		// Sports & Activities
		"⚽", "🏀", "🏈", "⚾", "🥎", "🎾", "🏐", "🏉", "🥏", "🎱", "🪀", "🏓", "🏸", "🏒", "🏑", "🥍", "🏏", "🪃", "🥅", "⛳",
		"🪁", "🤿", "🎣", "🤿", "🎿", "🛷", "🥌", "🎯", "🪀", "🪁", "🔫", "🎱", "🔮", "🪄", "🎮", "🕹️", "🎰", "🎲", "🧩", "🧸",
		// Hearts & Emotions
		"❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "💔", "❤️‍🔥", "❤️‍🩹", "❣️", "💕", "💞", "💓", "💗", "💖", "💘", "💝",
	}

	return emojis[rand.Intn(len(emojis))]
}
