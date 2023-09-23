package apis

import (
	"context"
	"fmt"
	"geocoding/pkg/container"
	"geocoding/pkg/forward"
	"os"

	"geocoding/pkg/proto"
	"log"
	"net"

	"github.com/jedib0t/go-pretty/table"
	"google.golang.org/grpc"
	"google.golang.org/grpc/health"
	"google.golang.org/grpc/health/grpc_health_v1"
	"google.golang.org/grpc/reflection"
	goproto "google.golang.org/protobuf/proto"
)

type opengeocodingServer struct {
	container *container.Container
	proto.UnimplementedOpenGeocodingServer
}

func (s *opengeocodingServer) Forward(ctx context.Context, request *proto.ForwardRequest) (*proto.ForwardResult, error) {
	forwardResult, err := forward.Forward(s.container, request.Address)
	if err != nil {
		return nil, err
	}
	return forwardResult, nil
}

type opengeocodingServerInternal struct {
	container *container.Container
	proto.UnimplementedOpenGeocodingInternalServer
}

func (s *opengeocodingServerInternal) RunQuery(ctx context.Context, request *proto.RunQueryRequest) (*proto.RunQueryResponse, error) {
	rows, err := s.container.Database.Worker.Query(request.Query)
	if err != nil {
		return nil, err
	}

	t := table.NewWriter()
	t.SetOutputMirror(os.Stdout)
	t.SetStyle(table.StyleColoredBright)

	cols, err := rows.Columns()
	if err != nil {
		return nil, err
	}

	var row table.Row
	for _, element := range cols {
		row = append(row, element)
	}
	t.AppendHeader(row)

	for rows.Next() {
		columns := make([]string, len(cols))
		columnPointers := make([]interface{}, len(cols))
		for i := range columns {
			columnPointers[i] = &columns[i]
		}

		rows.Scan(columnPointers...)

		var row table.Row
		for i := range cols {
			row = append(row, columns[i])
		}
		t.AppendRow(row)
	}
	t.Render()

	return &proto.RunQueryResponse{}, nil
}

func (s *opengeocodingServerInternal) RunBackgroundQuery(ctx context.Context, request *proto.RunBackgroundQueryRequest) (*proto.RunBackgroundQueryResponse, error) {
	err := s.container.Messaging.Publish("main:::opengeocoding:backgroundSave", request.Query)
	if err != nil {
		log.Println(err)
		return nil, err
	}

	return &proto.RunBackgroundQueryResponse{}, nil
}

func (s *opengeocodingServerInternal) InsertLocations(ctx context.Context, request *proto.InsertLocationsRequest) (*proto.InsertLocationsResponse, error) {
	if len(request.Locations) == 0 {
		return &proto.InsertLocationsResponse{}, nil
	}

	data, err := goproto.Marshal(request)
	if err != nil {
		return nil, fmt.Errorf("cannot marshal proto message to binary: %w", err)
	}

	err = s.container.Messaging.PublishBytes("main:::opengeocoding:insertDocuments", data)
	if err != nil {
		return nil, err
	}

	return &proto.InsertLocationsResponse{}, nil
}

func StartGrpc(container *container.Container) {

	port := 8091
	lis, err := net.Listen("tcp", fmt.Sprintf(":%d", port))
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	grpcServer := grpc.NewServer()
	grpc_health_v1.RegisterHealthServer(grpcServer, health.NewServer())
	proto.RegisterOpenGeocodingServer(grpcServer, &opengeocodingServer{
		container: container,
	})
	proto.RegisterOpenGeocodingInternalServer(grpcServer, &opengeocodingServerInternal{
		container: container,
	})
	reflection.Register(grpcServer)

	container.GracefulManager.OnShutdown(func() {
		grpcServer.GracefulStop()
	})

	// start gRPC server
	go func() {
		fmt.Println("Serving GRPC server on port", port)
		err := grpcServer.Serve(lis)
		if err != nil {
			log.Fatalln(err)
		}
	}()
}
