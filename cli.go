package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
)

func main() {
	id, err := strconv.Atoi(os.Args[1])
	if err != nil {
		fmt.Println(err.Error())
		os.Exit(1)
	}
	req, err := http.Get("https://search6.valk.sh/api?id=" + strconv.Itoa(id))
	if err != nil {
		fmt.Println(err.Error())
		os.Exit(2)
	}
	data, err := io.ReadAll(req.Body)
	if err != nil {
		fmt.Println(err.Error())
		os.Exit(3)
	}
	user := new(User)
	err = json.Unmarshal(data, user)
	if err != nil {
		fmt.Println(err.Error())
		os.Exit(4)
	}
	fmt.Printf("%s#%s (%d) is level %d\n", user.Username, user.Discriminator, user.ID, user.Level)
	if user.Level >= 5 {
		fmt.Printf("https://search6.valk.sh/card?id=%d <@%d>", user.ID, user.ID)
	}
}

type User struct {
	AvatarURL     string  `json:"avatar_url"`
	Level         int     `json:"level"`
	LevelProgress float64 `json:"level_progress"`
	Xp            int     `json:"xp"`
	ID            int64   `json:"id"`
	Username      string  `json:"username"`
	Discriminator string  `json:"discriminator"`
	Avatar        string  `json:"avatar"`
	MessageCount  int     `json:"message_count"`
	Rank          int     `json:"rank"`
}
