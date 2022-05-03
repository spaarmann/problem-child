FROM alpine:latest

COPY problem_child /opt/problem_child
CMD [ "/opt/problem_child/problem_child" ]
