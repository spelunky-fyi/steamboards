#!/bin/bash

git pull --rebase origin main && make docker-build && sudo systemctl restart steamboards.service
