package tree_sitter_pbd_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_pbd "github.com/whzard/punybuf//bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_pbd.Language())
	if language == nil {
		t.Errorf("Error loading Punybuf Definition grammar")
	}
}
