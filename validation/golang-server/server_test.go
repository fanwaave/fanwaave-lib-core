package servervalidation

import (
	"strings"
	"testing"

	public "github.com/fanwaave/fanwaave-lib-core/validation/golang"
)

func validContext() ServerRequestContext {
	return ServerRequestContext{
		Public: public.RequestMeta{
			RequestID: "req-1",
			TraceID:   "trace-1",
		},
		Actor: TrustedActor{
			UserID: "user-1",
			Roles:  []string{"reader"},
		},
		SourceIP: "127.0.0.1",
	}
}

func TestAcceptsBoundedServerValues(t *testing.T) {
	if err := Validate(validContext()); err != nil {
		t.Fatalf("expected valid context: %v", err)
	}
	command := InternalCommand{
		OperationID:    "alerts.create",
		IdempotencyKey: "idem-1",
		Context:        validContext(),
		Payload:        map[string]any{"message": "hello"},
	}
	if err := Validate(command); err != nil {
		t.Fatalf("expected valid command: %v", err)
	}
}

func TestRejectsInvalidTrustedActor(t *testing.T) {
	cases := []TrustedActor{
		{UserID: "", Roles: nil},
		{UserID: strings.Repeat("u", 129), Roles: nil},
		{UserID: "user-1", TenantID: strings.Repeat("t", 129), Roles: nil},
		{UserID: "user-1", Roles: []string{""}},
		{UserID: "user-1", Roles: []string{strings.Repeat("r", 129)}},
		{UserID: "user-1", Roles: make([]string, 65)},
	}
	for _, actor := range cases {
		if err := Validate(actor); err == nil {
			t.Fatalf("expected invalid actor: %#v", actor)
		}
	}
}

func TestRejectsInvalidServerContext(t *testing.T) {
	value := validContext()
	value.SourceIP = "not-an-ip"
	if err := Validate(value); err == nil {
		t.Fatal("expected invalid source IP")
	}

	value = validContext()
	value.Public.RequestID = ""
	if err := Validate(value); err == nil {
		t.Fatal("expected invalid nested public metadata")
	}
}

func TestRejectsInvalidInternalCommand(t *testing.T) {
	cases := []InternalCommand{
		{OperationID: "", Context: validContext()},
		{OperationID: strings.Repeat("o", 257), Context: validContext()},
		{OperationID: "alerts.create", IdempotencyKey: strings.Repeat("i", 129), Context: validContext()},
	}
	for _, command := range cases {
		if err := Validate(command); err == nil {
			t.Fatalf("expected invalid command: %#v", command)
		}
	}
}
