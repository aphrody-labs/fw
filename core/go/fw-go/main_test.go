package main

import (
	"encoding/base64"
	"strings"
	"testing"
)

func TestSha256Hex(t *testing.T) {
	got := Sha256Hex([]byte("fw"))
	if len(got) != 64 {
		t.Fatalf("expected 64 hex chars, got %d", len(got))
	}
}

func TestSshFingerprint(t *testing.T) {
	b64 := base64.StdEncoding.EncodeToString([]byte("fw-ed25519-test-pubkey-bytes"))
	fp, err := SshFingerprint("ssh-ed25519 " + b64 + " user@fw")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.HasPrefix(fp, "SHA256:") {
		t.Fatalf("expected SHA256 prefix, got %s", fp)
	}
}
