#!/bin/bash

# Install services
ssh pi@pi.iot.connieh.com "sudo apt-get install mosquitto nginx"

# Set up mosquitto
scp deploy_configs/mosquitto.conf pi@pi.iot.connieh.com:/etc/mosquitto/
ssh pi@pi.iot.connieh.com "sudo systemctl restart mosquitto.service"

# Set up nginx
scp deploy_configs/iot.conf pi@pi.iot.connieh.com:/etc/nginx/conf.d/
ssh pi@pi.iot.connieh.com "sudo nginx -s reload"

# Set up the home-server
scp deploy_configs/home-server.service pi@pi.iot.connieh.com:.config/systemd/user/home-server.service
ssh pi@pi.iot.connieh.com "systemctl --user enable home-server.service"
./deploy.sh
