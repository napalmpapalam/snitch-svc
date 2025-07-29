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

			// Track channel updates with actions
			type channelUpdate struct {
				channelID string
				action    string
				userID    string
			}
			var updates []channelUpdate

			// User left a voice channel
			if vs.ChannelID == "" && exists && cachedState.ChannelID != "" {
				// Check if we should track this event (for the channel they left)
				if vs.GuildID == cfg.Discord().TargetGuildID && contains(cfg.Discord().TrackedChannels, cachedState.ChannelID) {
					updates = append(updates, channelUpdate{
						channelID: cachedState.ChannelID,
						action:    "left",
						userID:    vs.UserID,
					})
				}

				// Remove from cache
				cache.mu.Lock()
				delete(cache.states, vs.UserID)
				cache.mu.Unlock()
			} else if vs.ChannelID != "" {
				// User joined or switched channels
				if exists && cachedState.ChannelID != "" && cachedState.ChannelID != vs.ChannelID {
					// User switched channels - they left the old channel
					if vs.GuildID == cfg.Discord().TargetGuildID && contains(cfg.Discord().TrackedChannels, cachedState.ChannelID) {
						updates = append(updates, channelUpdate{
							channelID: cachedState.ChannelID,
							action:    "left",
							userID:    vs.UserID,
						})
					}
				}

				// Update cache first
				cache.mu.Lock()
				cache.states[vs.UserID] = &discordgo.VoiceState{
					UserID:    vs.UserID,
					ChannelID: vs.ChannelID,
					GuildID:   vs.GuildID,
				}
				cache.mu.Unlock()

				// Check if the new channel should be tracked - they joined this channel
				if shouldTrackVoiceEvent(cfg, vs) {
					updates = append(updates, channelUpdate{
						channelID: vs.ChannelID,
						action:    "joined",
						userID:    vs.UserID,
					})
				}
			}

			// Send updates for all affected channels
			for _, update := range updates {
				sendChannelMembersList(cfg, s, tgBot, update.channelID, update.action, update.userID)
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
	msg.ParseMode = "HTML"
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

func getVoiceChannelMembers(s *discordgo.Session, guildID, channelID string) []string {
	var members []string

	// Get all voice states from cache
	cache.mu.RLock()
	defer cache.mu.RUnlock()

	for userID, voiceState := range cache.states {
		if voiceState.GuildID == guildID && voiceState.ChannelID == channelID {
			displayName := formatUserDisplayName(s, guildID, userID)
			members = append(members, displayName)
		}
	}

	return members
}

func sendChannelMembersList(cfg config.Config, s *discordgo.Session, bot *tgbotapi.BotAPI, channelID string, action string, userID string) {
	channel, err := s.Channel(channelID)
	if err != nil {
		cfg.Log().Error("failed to get channel", logan.F{
			"error":      err,
			"channel_id": channelID,
		})
		return
	}

	members := getVoiceChannelMembers(s, cfg.Discord().TargetGuildID, channelID)
	timestamp := time.Now().Format("2006-01-02 15:04:05")

	// Get the display name of the user who triggered the action
	var actionLine string
	if action != "" && userID != "" {
		userDisplayName := formatUserDisplayName(s, cfg.Discord().TargetGuildID, userID)
		if action == "joined" {
			actionLine = fmt.Sprintf("➡️ <b>%s</b> just joined\n\n", userDisplayName)
		} else if action == "left" {
			actionLine = fmt.Sprintf("⬅️ <b>%s</b> just left\n\n", userDisplayName)
		}
	}

	var msg string
	if len(members) == 0 {
		msg = fmt.Sprintf("📍 <b>Channel: %s</b>\n🕐 %s\n%s🔇 Voice channel is now empty", channel.Name, timestamp, actionLine)
	} else {
		membersList := ""
		for _, member := range members {
			membersList += fmt.Sprintf("  • %s\n", member)
		}
		msg = fmt.Sprintf("📍 <b>Channel: %s</b>\n🕐 %s\n%s🎤 <b>Active Members (%d):</b>\n%s", channel.Name, timestamp, actionLine, len(members), membersList)
	}

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
