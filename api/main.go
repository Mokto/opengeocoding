package main

import (
	context "context"
	"database/sql"
	"fmt"
	"log"
	"net"

	"geocoding/pkg/forward"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/proto"

	_ "github.com/go-sql-driver/mysql"
	grpc "google.golang.org/grpc"
	"google.golang.org/grpc/health"
	"google.golang.org/grpc/health/grpc_health_v1"
	"google.golang.org/grpc/reflection"
)

type opengeocodingServer struct {
	database *sql.DB
	proto.UnimplementedOpenGeocodingServer
}

func (s *opengeocodingServer) Forward(ctx context.Context, request *proto.ForwardRequest) (*proto.ForwardResponse, error) {
	location, err := forward.Forward(s.database, request.Address)
	if err != nil {
		return nil, err
	}
	return &proto.ForwardResponse{
		Location: location,
	}, nil
}

type opengeocodingServerInternal struct {
	database *sql.DB
	proto.UnimplementedOpenGeocodingInternalServer
}

func (s *opengeocodingServerInternal) RunQuery(ctx context.Context, request *proto.RunQueryRequest) (*proto.RunQueryResponse, error) {
	// rows, err := s.database.Exec(request.Query)
	// if err != nil {
	// 	return nil, err
	// }

	// rows.

	// defer rows.Close()

	// for rows.Next() {
	// 	var name interface{}
	// 	if err := rows.Scan(&name); err != nil {
	// 		log.Fatal(err)
	// 	}
	// 	fmt.Println("COUCOU")
	// }

	return &proto.RunQueryResponse{
		// Result: fmt.Sprintln(res),
	}, nil
}

func main() {
	database := manticoresearch.InitDatabase()
	err := database.Ping()
	if err != nil {
		panic(err)
	}

	port := 8091
	lis, err := net.Listen("tcp", fmt.Sprintf("localhost:%d", port))
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}
	grpcServer := grpc.NewServer()
	grpc_health_v1.RegisterHealthServer(grpcServer, health.NewServer())
	proto.RegisterOpenGeocodingServer(grpcServer, &opengeocodingServer{
		database: database,
	})
	proto.RegisterOpenGeocodingInternalServer(grpcServer, &opengeocodingServerInternal{
		database: database,
	})
	reflection.Register(grpcServer)
	fmt.Println("Serving GRPC server on port", port)
	grpcServer.Serve(lis)
}
