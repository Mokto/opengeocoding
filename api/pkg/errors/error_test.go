package errors_test

import (
	"errors"
	"fmt"
	"testing"

	errorscommon "geocoding/pkg/errors"
)

type ErrorTestCase struct {
	Error              errorscommon.Error
	ExpectedMessage    string
	ExpectedStackTrace []errorscommon.Frame
}

func TestCustomError(t *testing.T) {
	err := errors.New("some error")
	frames := []errorscommon.Frame{
		{
			Func: "main.foo",
			Line: 42,
			Path: "/src/github.com/john/doe/foobar.go",
		},
		{
			Func: "main.bar",
			Line: 43,
			Path: "/src/github.com/john/doe/bazqux.go",
		},
	}
	customErr := errorscommon.CustomError(err, frames)
	message := customErr.Error()
	if message != err.Error() {
		t.Errorf(
			"customErr.Error() = %#v; want %#v",
			message, err.Error(),
		)
	}
	unwrapped := customErr.Unwrap()
	if unwrapped != err {
		t.Errorf(
			"customErr.Unwrap() = %#v; want %#v",
			unwrapped, err,
		)
	}
	stackTrace := customErr.StackTrace()
	if len(stackTrace) != len(frames) {
		t.Errorf(
			"len(customErr.StackTrace()) = %#v; want %#v",
			len(stackTrace), len(frames),
		)
	}
	for i, frame := range frames {
		if stackTrace[i] != frame {
			t.Errorf(
				"customErr.StackTrace()[%#v] = %#v; want %#v",
				i, stackTrace[i], frame,
			)
		}
	}
}

func TestErrorNil(t *testing.T) {
	wrapped := wrapError(nil)
	if wrapped != nil {
		t.Errorf(
			"wrapped = %#v; want nil",
			wrapped,
		)
	}
}

func TestFrameString(t *testing.T) {
	frame := errorscommon.Frame{
		Func: "main.read",
		Line: 1337,
		Path: "/src/github.com/john/doe/foobar.go",
	}
	expected := "/src/github.com/john/doe/foobar.go:1337 main.read()"
	if frame.String() != expected {
		t.Errorf(
			"frame.String() = %#v; want %#v",
			frame.String(), expected,
		)
	}
}

func TestStackTraceNotInstance(t *testing.T) {
	err := errors.New("regular error")
	if errorscommon.StackTrace(err) != nil {
		t.Errorf(
			"errorscommon.StackTrace(%#v) = %#v; want %#v",
			err, errorscommon.StackTrace(err), nil,
		)
	}
}

type UnwrapTestCase struct {
	Error error
	Wrap  bool
}

func TestUnwrap(t *testing.T) {
	cases := []UnwrapTestCase{
		{
			Error: nil,
		},
		{
			Error: fmt.Errorf("some error #%d", 9),
			Wrap:  false,
		},
		{
			Error: fmt.Errorf("some error #%d", 9),
			Wrap:  true,
		},
	}

	for i, c := range cases {
		err := c.Error
		if c.Wrap {
			err = errorscommon.Wrap(err)
		}
		unwrappedError := errorscommon.Unwrap(err)
		if unwrappedError != c.Error {
			t.Errorf(
				"errorscommon.Unwrap(cases[%#v].Error) = %#v; want %#v",
				i, unwrappedError, c.Error,
			)
		}
	}
}

func wrapError(err error) error {
	return errorscommon.Wrap(err)
}
