package main

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/alexflint/go-arg"
)

func Append(f *os.File, toAppend []string) (err error) {
	_, err = f.Seek(0, os.SEEK_END)
	if err != nil {
		return
	}

	bw := bufio.NewWriter(f)

	if !EndsWithNewLine(f) {
		_ = bw.WriteByte(byte('\n'))
	}
	for _, s := range toAppend {
		_, _ = bw.WriteString(s)
	}
	_ = bw.WriteByte(byte('\n'))
	return bw.Flush()
}

func OpenAndAppend(path string, toAppend []string) (err error) {
	defer func() {
		if err != nil {
			err = fmt.Errorf(`file "%s": %w`, path, err)
		}
	}()
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o644)
	if err != nil {
		return
	}
	defer f.Close()
	err = Append(f, toAppend)
	return
}

func EndsWithNewLine(f *os.File) bool {
	var endln [1]byte
	cur, err := f.Seek(0, os.SEEK_CUR)
	if err != nil {
		return false
	}
	defer f.Seek(cur, os.SEEK_SET)
	_, _ = f.Seek(-1, os.SEEK_END)
	_, _ = f.Read(endln[:])
	return endln[0] == byte('\n')
}

var flagsPath = regexp.MustCompile(`^((?:(?:#include )|(?:\(\?.\))*)) *(.+)$`)

const (
	includeDirective = "#include "
	ignoreSyncF      = ".stignore_sync"
	ignoreF          = ".stignore"
)

func Prompt() (bool, error) {
	r := bufio.NewReader(os.Stdin)
	first := true
	for {
		if first {
			fmt.Printf("%s exists, but not included in %s. Include it ([Y]/n)? ", ignoreSyncF, ignoreF)
		} else {
			fmt.Print("Wrong input. ([Y]/n): ")
		}
		text, err := r.ReadString('\n')
		if err != nil && err != io.EOF {
			return false, fmt.Errorf("prompt failed: %w", err)
		}
		text = strings.TrimSpace(text)
		if len(text) <= 1 {
			return true, nil
		}
		switch text[0] {
		case 'y', 'Y':
			return true, nil
		case 'n', 'N':
			return false, nil
		default:
			first = false
		}
	}

}

func WriteToSynced(stDir string, shouldPrompt bool) (err error) {
	path := filepath.Join(stDir, ignoreF)
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o644)
	if err != nil {
		return fmt.Errorf(`file "%s": %w`, path, err)
	}
	defer f.Close()

	br := bufio.NewReader(f)
	var line string
	isIncluded := false

	for err == nil {
		line, err = br.ReadString(byte('\n'))
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "//") {
			continue
		}
		m := flagsPath.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		if m[1] == includeDirective && m[2] == ignoreSyncF {
			isIncluded = true
			break
		}
	}
	var prompt bool
	if !isIncluded && shouldPrompt {
		prompt, err = Prompt()
		if err != nil {
			return
		}
	}

	if isIncluded || !shouldPrompt || prompt {
		err = OpenAndAppend(filepath.Join(stDir, ignoreSyncF), args.Patterns)
		if err != nil {
			return
		}

		if !isIncluded {
			if EndsWithNewLine(f) {
				_, err = f.Write([]byte(includeDirective + ignoreSyncF + "\n"))
			} else {
				_, err = f.Write([]byte("\n" + includeDirective + ignoreSyncF + "\n"))
			}
		}
	} else {
		err = Append(f, args.Patterns)
	}
	if err != nil {
		err = fmt.Errorf(`file "%s": %w`, path, err)
	}
	return
}

func FindParentSyncthingDir() (stDir, relPath string) {
	var cur string

	cwd, err := os.Getwd()
	if err != nil {
		return
	}
	cwd, err = filepath.Abs(cwd)
	if err != nil {
		return
	}

	for cur = cwd; cur != "." && cur != string(filepath.Separator); cur = filepath.Dir(cur) {
		info, err := os.Stat(filepath.Join(cur, ".stfolder"))
		if err != nil {
			continue
		}
		if info.IsDir() {
			break
		}
	}
	if cur == "." || cur == string(filepath.Separator) {
		return
	}
	return cur, cwd[len(cur):]
}

func do() (stDir string, err error) {
	defer func() {
		if r := recover(); r != nil {
			err = fmt.Errorf("critical: %v", r)
		}
	}()

	stDir, relPath := FindParentSyncthingDir()
	if stDir == "" {
		err = fmt.Errorf("current working dir is not inside of syncthing folder")
		return
	}

	if !args.Absolute {
		// separate flags/#include and paths and prepend rel path to paths
		for i := range args.Patterns {
			args.Patterns[i] = strings.TrimSpace(args.Patterns[i])
			if strings.HasPrefix(args.Patterns[i], "//") {
				continue
			}
			m := flagsPath.FindStringSubmatch(args.Patterns[i])
			if m == nil {
				err = fmt.Errorf(`incorrect pattern: "%s"`, args.Patterns[i])
				return
			}
			if m[2] == "" {
				continue
			}
			args.Patterns[i] = m[1] + filepath.Join(relPath, m[2])
		}
	}

	shouldPromptIfMissing := false
	if !(args.Local || args.Synced) {
		stat, err := os.Stat(filepath.Join(stDir, ignoreSyncF))
		if err == nil && !stat.IsDir() {
			args.Synced = true
			shouldPromptIfMissing = true
		} else {
			args.Local = true
		}
	}
	switch {
	case args.Synced:
		err = WriteToSynced(stDir, shouldPromptIfMissing)
	case args.Local:
		err = OpenAndAppend(filepath.Join(stDir, ignoreF), args.Patterns)
	}
	return
}

type argsS struct {
	Local    bool `arg:"-l,--local" help:"add patterns to .stignore (not synced)"`
	Synced   bool `arg:"-s,--synced" help:"add patterns to .stignore_sync (synced across devices)"`
	Absolute bool `arg:"-a,--absolute" help:"don't prepend relative path from syncthing folder to CWD"`

	Patterns []string `arg:"positional,required" placeholder:"PATTERN" help:"pattern to add"`
}

func (argsS) Description() string {
	return (`stignore v0.0.1

Adds Syncthing ignore patterns (https://docs.syncthing.net/users/ignoring) to parent syncthing folder of the working directory.

By default prepends relative path from syncthing folder to CWD to patterns, disabled using --absolute.

By default adds patterns to .stignore_sync if it exists. By my personal convention this file is #included in .stignore on each device and synced using Syncthing.
If it's missing - adds patterns to .stignore. You can override this behaviour by using --local or --synced flags.
`)
}

var args argsS

func main() {
	arg.MustParse(&args)
	stDir, err := do()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %s\n", err)
		os.Exit(1)
	}
	fmt.Printf("Patterns added to syncthing dir \"%s\"\n", stDir)
}
