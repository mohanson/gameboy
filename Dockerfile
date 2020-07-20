ARG BASE_IMAGE=frolvlad/alpine-rust
FROM $BASE_IMAGE as builder

COPY res res
COPY src src
COPY examples examples
ADD Cargo.toml .
ADD Cargo.lock .

RUN cargo build --release --no-default-features --features tty

EXPOSE 2222

run cp target/release/gameboy /usr/bin/
RUN apk add --no-cache openssh \
  && sed -i s/#PermitRootLogin.*/PermitRootLogin\ yes/ /etc/ssh/sshd_config \
  && echo "root:root" | chpasswd
run echo "ForceCommand /usr/bin/http-gameboy.sh" >> /etc/ssh/sshd_config
run ssh-keygen -A
COPY http-gameboy.sh /usr/bin/
ENTRYPOINT ["/usr/sbin/sshd", "-D", "-e", "-p", "2222"]

