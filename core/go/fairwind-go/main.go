package main

import (
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"runtime"
	"strings"
)

type StatusPayload struct {
	Ok      bool   `json:"ok"`
	Product string `json:"product"`
	Lang    string `json:"lang"`
	GoVer   string `json:"goVersion"`
	Os      string `json:"os"`
	Arch    string `json:"arch"`
}

func Sha256Hex(data []byte) string {
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}

func SshFingerprint(pubKeyLine string) (string, error) {
	fields := strings.Fields(pubKeyLine)
	if len(fields) < 2 {
		return "", fmt.Errorf("invalid ssh public key")
	}
	raw, err := base64.StdEncoding.DecodeString(fields[1])
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(raw)
	return "SHA256:" + base64.RawStdEncoding.EncodeToString(sum[:]), nil
}

func main() {
	cmd := "status"
	if len(os.Args) > 1 {
		cmd = os.Args[1]
	}
	switch cmd {
	case "hash":
		input := ""
		if len(os.Args) > 2 {
			input = os.Args[2]
		}
		out, _ := json.Marshal(map[string]any{"ok": true, "sha256": Sha256Hex([]byte(input))})
		fmt.Println(string(out))
	default:
		out, _ := json.Marshal(StatusPayload{
			Ok:      true,
			Product: "fairwind",
			Lang:    "go",
			GoVer:   runtime.Version(),
			Os:      runtime.GOOS,
			Arch:    runtime.GOARCH,
		})
		fmt.Println(string(out))
	}
}
