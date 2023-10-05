// Package errors makes error output more informative.
// It adds stack trace to error and can display error with source fragments.
package errors

import (
	"fmt"
	"runtime"
)

// DefaultCap is a default cap for frames array.
// It can be changed to number of expected frames
// for purpose of performance optimisation.
var DefaultCap = 20

// Error is an error with stack trace.
type Error interface {
	Error() string
	StackTrace() []Frame
	Unwrap() error
	UnwrapContext() map[string]interface{}
	AddContext(key string, value interface{}) Error
	SetType(isPanic bool, errorType string) Error
	GetType() (isPanic bool, errorType string)
}

type errorData struct {
	// err contains original error.
	err       error
	errorType string
	context   map[string]interface{}
	// frames contains stack trace of an error.
	frames  []Frame
	isPanic bool
}

// CustomError creates an error with provided frames.
func CustomError(err error, frames []Frame) Error {
	return &errorData{
		err:    err,
		frames: frames,
	}
}

// Errorf creates new error with stacktrace and formatted message.
// Formatting works the same way as in fmt.Errorf.
func Errorf(message string, args ...interface{}) Error {
	return trace(fmt.Errorf(message, args...), 2)
}

// New creates new error with stacktrace.
func New(message string) Error {
	return trace(fmt.Errorf(message), 2)
}

// Wrap adds stacktrace to existing error.
func Wrap(err error) Error {
	if err == nil {
		return nil
	}
	e, ok := err.(Error)
	if ok {
		return e
	}
	return trace(err, 2)
}

// Unwrap returns the original error.
func Unwrap(err error) error {
	if err == nil {
		return nil
	}
	e, ok := err.(Error)
	if !ok {
		return err
	}
	return e.Unwrap()
}

// UnwrapContext returns the error context.
func UnwrapContext(err error) map[string]interface{} {
	if err == nil {
		return nil
	}
	e, ok := err.(Error)
	if !ok {
		return nil
	}
	return e.UnwrapContext()
}

// GetType returns what's saved by SetType
func GetType(err error) (bool, string) {
	if err == nil {
		return false, "Error"
	}
	e, ok := err.(Error)
	if !ok {
		return false, "Error"
	}
	return e.GetType()
}

// Error returns error message.
func (e *errorData) Error() string {
	return e.err.Error()
}

// StackTrace returns stack trace of an error.
func (e *errorData) StackTrace() []Frame {
	return e.frames
}

// Unwrap returns the original error.
func (e *errorData) Unwrap() error {
	return e.err
}

// Unwrap returns the original error.
func (e *errorData) UnwrapContext() map[string]interface{} {
	return e.context
}
func (e *errorData) AddContext(key string, value interface{}) Error {
	if e.context == nil {
		e.context = map[string]interface{}{}
	}
	e.context[key] = value
	return e
}

func (e *errorData) SetType(isPanic bool, errorType string) Error {
	e.isPanic = isPanic
	e.errorType = errorType
	return e
}

func (e *errorData) GetType() (isPanic bool, errorType string) {
	errType := "Error"
	if e.errorType != "" {
		errType = e.errorType
	}
	return e.isPanic, errType
}

// Frame is a single step in stack trace.
type Frame struct {
	// Func contains a function name.
	Func string
	// Path contains a file path.
	Path string
	// Line contains a line number.
	Line int
}

// StackTrace returns stack trace of an error.
// It will be empty if err is not of type Error.
func StackTrace(err error) []Frame {
	e, ok := err.(Error)
	if !ok {
		return nil
	}
	return e.StackTrace()
}

// String formats Frame to string.
func (f Frame) String() string {
	return fmt.Sprintf("%s:%d %s()", f.Path, f.Line, f.Func)
}

func trace(err error, skip int) Error {
	frames := make([]Frame, 0, DefaultCap)
	for {
		pc, path, line, ok := runtime.Caller(skip)
		if !ok {
			break
		}
		fn := runtime.FuncForPC(pc)
		frame := Frame{
			Func: fn.Name(),
			Line: line,
			Path: path,
		}
		frames = append(frames, frame)
		skip++
	}
	errData := &errorData{
		err:    err,
		frames: frames,
	}
	return errData
}
