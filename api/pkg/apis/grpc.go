package apis

import (
	"context"
	"fmt"
	"geocoding/pkg/container"
	"geocoding/pkg/forward"

	"geocoding/pkg/proto"
	"log"
	"net"

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
