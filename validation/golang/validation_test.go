package validation

import (
	"strings"
	"testing"
)

func TestRequestMetaBoundaries(t *testing.T) {
	valid := []RequestMeta{
		{RequestID: "req-1", TraceID: "trace-1"},
		{RequestID: strings.Repeat("r", 128), TraceID: strings.Repeat("t", 128), Locale: strings.Repeat("l", 64)},
	}
	for _, value := range valid {
		if err := Validate(value); err != nil {
			t.Fatalf("expected valid request metadata: %v", err)
		}
	}

	invalid := []RequestMeta{
		{RequestID: "", TraceID: "trace-1"},
		{RequestID: strings.Repeat("r", 129), TraceID: "trace-1"},
		{RequestID: "req-1", TraceID: ""},
		{RequestID: "req-1", TraceID: strings.Repeat("t", 129)},
		{RequestID: "req-1", TraceID: "trace-1", Locale: "e"},
		{RequestID: "req-1", TraceID: "trace-1", Locale: strings.Repeat("l", 65)},
	}
	for _, value := range invalid {
		if err := Validate(value); err == nil {
			t.Fatalf("expected invalid request metadata: %#v", value)
		}
	}
}

func TestPageQueryBoundaries(t *testing.T) {
	valid := []PageQuery{
		{Limit: 1},
		{Limit: 100, Cursor: strings.Repeat("c", 512)},
	}
	for _, value := range valid {
		if err := Validate(value); err != nil {
			t.Fatalf("expected valid page query: %v", err)
		}
	}

	invalid := []PageQuery{
		{Limit: 0},
		{Limit: 101},
		{Limit: 50, Cursor: strings.Repeat("c", 513)},
	}
	for _, value := range invalid {
		if err := Validate(value); err == nil {
			t.Fatalf("expected invalid page query: %#v", value)
		}
	}
}

func TestProblemDetailsBoundaries(t *testing.T) {
	base := ProblemDetails{
		Type:      "https://example.test/problems/invalid",
		Title:     "Invalid request",
		Status:    400,
		RequestID: "req-1",
	}
	if err := Validate(base); err != nil {
		t.Fatalf("expected valid problem details: %v", err)
	}
	maximum := base
	maximum.Status = 599
	maximum.Detail = strings.Repeat("d", 4096)
	if err := Validate(maximum); err != nil {
		t.Fatalf("expected maximum problem details: %v", err)
	}

	invalid := []ProblemDetails{
		func() ProblemDetails { value := base; value.Status = 399; return value }(),
		func() ProblemDetails { value := base; value.Status = 600; return value }(),
		func() ProblemDetails { value := base; value.Type = ""; return value }(),
		func() ProblemDetails { value := base; value.Title = ""; return value }(),
		func() ProblemDetails { value := base; value.Detail = strings.Repeat("d", 4097); return value }(),
	}
	for _, value := range invalid {
		if err := Validate(value); err == nil {
			t.Fatalf("expected invalid problem details: %#v", value)
		}
	}
}

func TestDecodeRejectsUnknownMissingAndTrailingData(t *testing.T) {
	cases := [][]byte{
		[]byte(`{"requestId":"req-1","traceId":"trace-1","userId":"client-supplied"}`),
		[]byte(`{"requestId":"req-1"}`),
		[]byte(`{"requestId":"req-1","traceId":"trace-1"} {"requestId":"req-2","traceId":"trace-2"}`),
	}
	for _, data := range cases {
		if _, err := DecodeAndValidate[RequestMeta](data); err == nil {
			t.Fatalf("expected decode failure for %s", data)
		}
	}
}

func TestDecodePreservesExactIdentifierText(t *testing.T) {
	value, err := DecodeAndValidate[RequestMeta]([]byte(`{"requestId":" req-1 ","traceId":" trace-1 "}`))
	if err != nil {
		t.Fatalf("decode request metadata: %v", err)
	}
	if value.RequestID != " req-1 " || value.TraceID != " trace-1 " {
		t.Fatalf("decoder normalized identifiers: %#v", value)
	}
}
