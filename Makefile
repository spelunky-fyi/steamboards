docker-build:
	docker build -t steamboards:docker .

docker-run:
	docker run --name=steamboards.service --rm -it -p 127.0.0.1:16000:16000 steamboards:docker

docker-bash:
	docker exec -it steamboards.service /bin/bash
