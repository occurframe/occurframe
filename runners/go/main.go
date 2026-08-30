// Protocol-v2 adapter for the three Go configurations measured in Phase II RC1.
package main

import (
	"bufio"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"os"
	"runtime"
	"strings"
	"time"

	cronv3 "github.com/robfig/cron/v3"
	rrule "github.com/teambition/rrule-go"
)

const protocol = "2.0"

type vector struct {
	ID            string `json:"id"`
	CorpusVersion string `json:"corpus_version"`
	Operation     string `json:"operation"`
	Input         struct {
		Expr    string   `json:"expr"`
		Start   string   `json:"start"`
		Count   int      `json:"count"`
		Zone    *string  `json:"zone"`
		ICS     string   `json:"ics"`
		Between []string `json:"between"`
	} `json:"input"`
}

type caseMessage struct {
	Message         string `json:"message"`
	ProtocolVersion string `json:"protocol_version"`
	RequestID       string `json:"request_id"`
	Vector          vector `json:"vector"`
}

type diagnostic struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

type outcome struct {
	Type        string      `json:"type"`
	Occurrences *[]string   `json:"occurrences,omitempty"`
	Diagnostic  *diagnostic `json:"diagnostic,omitempty"`
}

type engine struct {
	name       string
	version    string
	provenance string
	operations []string
	dialects   []string
	run        func(vector) ([]string, error)
}

type enginePanic struct{ value any }

func (panicValue enginePanic) Error() string {
	return fmt.Sprintf("native panic: %v", panicValue.value)
}

func emit(writer *bufio.Writer, value any) error {
	if err := json.NewEncoder(writer).Encode(value); err != nil {
		return err
	}
	return writer.Flush()
}

func offString(value time.Time) string {
	_, offset := value.Zone()
	sign := "+"
	if offset < 0 {
		sign = "-"
		offset = -offset
	}
	return fmt.Sprintf("%s%02d:%02d", sign, offset/3600, (offset%3600)/60)
}

func formatZoned(value time.Time) string {
	return value.Format("2006-01-02T15:04:05") + offString(value) + "|" +
		value.UTC().Format("2006-01-02T15:04:05") + "Z"
}

func formatNaive(value time.Time) string { return value.Format("2006-01-02T15:04:05") }

func tzdbProvenance() map[string]any {
	for _, path := range []string{"/usr/share/zoneinfo/tzdata.zi", "/usr/lib/zoneinfo/tzdata.zi"} {
		contents, err := os.ReadFile(path)
		if err == nil {
			line := strings.SplitN(string(contents), "\n", 2)[0]
			return map[string]any{"source": "system zoneinfo", "release_kind": "exact",
				"release": strings.TrimPrefix(strings.TrimSpace(line), "# version ")}
		}
	}
	return map[string]any{"source": "system zoneinfo", "release_kind": "unknown"}
}

func startTime(item vector) (time.Time, error) {
	location := time.UTC
	if item.Input.Zone != nil {
		loaded, err := time.LoadLocation(*item.Input.Zone)
		if err != nil {
			return time.Time{}, err
		}
		location = loaded
	}
	return time.ParseInLocation("2006-01-02T15:04:05", item.Input.Start, location)
}

func runRobfig(item vector, seconds bool) (output []string, resultError error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			resultError = enginePanic{recovered}
		}
	}()
	expression := item.Input.Expr
	if item.Input.Zone != nil {
		expression = "TZ=" + *item.Input.Zone + " " + expression
	}
	var schedule cronv3.Schedule
	var err error
	if seconds {
		parser := cronv3.NewParser(cronv3.Second | cronv3.Minute | cronv3.Hour |
			cronv3.Dom | cronv3.Month | cronv3.Dow | cronv3.Descriptor)
		schedule, err = parser.Parse(expression)
	} else {
		schedule, err = cronv3.ParseStandard(expression)
	}
	if err != nil {
		return nil, err
	}
	current, err := startTime(item)
	if err != nil {
		return nil, err
	}
	deadline := current.AddDate(60, 0, 0)
	for index := 0; index < item.Input.Count; index++ {
		following := schedule.Next(current)
		if following.IsZero() || following.After(deadline) {
			break
		}
		if item.Input.Zone != nil {
			output = append(output, formatZoned(following))
		} else {
			output = append(output, formatNaive(following))
		}
		current = following
	}
	return output, nil
}

func runRRule(item vector) (output []string, resultError error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			resultError = enginePanic{recovered}
		}
	}()
	set, err := rrule.StrToRRuleSet(item.Input.ICS)
	if err != nil {
		return nil, err
	}
	zoned := item.Input.Zone != nil
	location := time.UTC
	if zoned {
		location, err = time.LoadLocation(*item.Input.Zone)
		if err != nil {
			return nil, err
		}
	}
	if item.Operation == "rrule.between" {
		first, firstError := time.ParseInLocation("2006-01-02T15:04:05", item.Input.Between[0], location)
		last, lastError := time.ParseInLocation("2006-01-02T15:04:05", item.Input.Between[1], location)
		if firstError != nil || lastError != nil {
			return nil, errors.Join(firstError, lastError)
		}
		for _, value := range set.Between(first, last, false) {
			if zoned {
				output = append(output, formatZoned(value.In(location)))
			} else {
				output = append(output, formatNaive(value))
			}
		}
		return output, nil
	}
	next := set.Iterator()
	for index := 0; index < item.Input.Count; index++ {
		value, ok := next()
		if !ok {
			break
		}
		if zoned {
			output = append(output, formatZoned(value.In(location)))
		} else {
			output = append(output, formatNaive(value))
		}
	}
	return output, nil
}

func selectedEngine(name string) (engine, error) {
	const robfigCommit = "git robfig/cron@bc59245fe10efaed9d51b56900192527ed733435"
	switch name {
	case "robfig-cron":
		return engine{name, "v3.0.1", robfigCommit, []string{"cron.next", "cron.parse"},
			[]string{"cron.robfig-v3-standard@1"}, func(item vector) ([]string, error) { return runRobfig(item, false) }}, nil
	case "robfig-cron[seconds]":
		return engine{name, "v3.0.1", robfigCommit, []string{"cron.next", "cron.parse"},
			[]string{"cron.robfig-v3-seconds@1"}, func(item vector) ([]string, error) { return runRobfig(item, true) }}, nil
	case "rrule-go":
		return engine{name, "v1.8.2", "git teambition/rrule-go@e74d163475cf1ca1fd019752c5c41ea1f472d4c5",
			[]string{"rrule.expand", "rrule.parse", "rrule.between"}, []string{}, runRRule}, nil
	default:
		return engine{}, fmt.Errorf("unknown engine %q", name)
	}
}

func contains(values []string, value string) bool {
	for _, candidate := range values {
		if candidate == value {
			return true
		}
	}
	return false
}

func main() {
	engineName := flag.String("engine", "", "immutable engine configuration")
	flag.Parse()
	selected, err := selectedEngine(*engineName)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	writer := bufio.NewWriter(os.Stdout)
	hello := map[string]any{
		"message": "hello", "protocol_version": protocol,
		"runner":       map[string]any{"name": "occurframe-go-runner", "version": "2.0.0", "provenance": "source:runners/go/main.go"},
		"engine":       map[string]any{"name": selected.name, "version": selected.version, "provenance": selected.provenance},
		"runtime":      map[string]any{"language": "Go", "runtime": "gc", "version": runtime.Version()},
		"capabilities": selected.operations, "dialect_ids": selected.dialects,
		"semantic_profile_claims": map[string]any{}, "tzdb_provenance": tzdbProvenance(),
	}
	if err := emit(writer, hello); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 64*1024), 8*1024*1024)
	for scanner.Scan() {
		if strings.TrimSpace(scanner.Text()) == "" {
			continue
		}
		var message caseMessage
		if err := json.Unmarshal(scanner.Bytes(), &message); err != nil || message.Message != "case" || message.ProtocolVersion != protocol {
			fmt.Fprintln(os.Stderr, "invalid protocol-v2 case")
			os.Exit(1)
		}
		if err := emit(writer, map[string]any{"message": "started", "protocol_version": protocol, "request_id": message.RequestID}); err != nil {
			os.Exit(1)
		}
		operation := message.Vector.Operation
		probe := operation
		if operation == "cron.parse" {
			probe = "cron.next"
		} else if operation == "rrule.parse" {
			probe = "rrule.expand"
		}
		var terminal outcome
		if !contains(selected.operations, operation) && !contains(selected.operations, probe) {
			terminal = outcome{Type: "unsupported", Diagnostic: &diagnostic{"unsupported_operation", selected.name + " does not implement " + operation}}
		} else {
			occurrences, runError := selected.run(message.Vector)
			if runError == nil {
				if strings.HasSuffix(operation, ".parse") {
					terminal = outcome{Type: "accepted"}
				} else {
					if occurrences == nil {
						occurrences = []string{}
					}
					terminal = outcome{Type: "occurrences", Occurrences: &occurrences}
				}
			} else {
				var panicError enginePanic
				if errors.As(runError, &panicError) {
					terminal = outcome{Type: "engine_error", Diagnostic: &diagnostic{"native_panic", runError.Error()}}
				} else {
					terminal = outcome{Type: "rejection", Diagnostic: &diagnostic{"native_rejection", runError.Error()}}
				}
			}
		}
		result := map[string]any{"message": "result", "protocol_version": protocol,
			"request_id": message.RequestID, "outcome": terminal, "warnings": []any{}}
		if err := emit(writer, result); err != nil {
			os.Exit(1)
		}
	}
	if err := scanner.Err(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
