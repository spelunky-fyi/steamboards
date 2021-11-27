import logging
import os
import struct

from dotenv import load_dotenv
from gevent.server import StreamServer
from steam.client import SteamClient
from steam.enums import EResult

load_dotenv("steamboards.env")

# setup logging
logging.basicConfig(format="%(asctime)s | %(message)s", level=logging.INFO)
LOG = logging.getLogger()

USERNAME = os.environ["STEAM_USERNAME"]
PASSWORD = os.environ["STEAM_PASSWORD"]

APP_ID = 239350
LEADERBOARD_FORMAT_STR = "{} DAILY"

client = SteamClient()


@client.on("error")
def handle_error(result):
    LOG.info("Logon result: %s", repr(result))


@client.on("channel_secured")
def send_login():
    if client.relogin_available:
        client.relogin()


@client.on("connected")
def handle_connected():
    LOG.info("Connected to %s", client.current_server_addr)


@client.on("reconnect")
def handle_reconnect(delay):
    LOG.info("Reconnect in %ds...", delay)


@client.on("disconnected")
def handle_disconnect():
    LOG.info("Disconnected.")

    if client.relogin_available:
        LOG.info("Reconnecting...")
        client.reconnect(maxdelay=30)


@client.on("logged_on")
def handle_after_logon():
    LOG.info("Logged on as: %s", client.user.name)


class RequestHandler:
    VALID_COMMANDS = set([b"FETCH", b"INFO"])

    def __init__(self, steam_client):
        self.steam_client = steam_client

    def handle_info(self, daily, socket):

        LOG.info("Received INFO for %s", daily)
        leaderboard = self.steam_client.get_leaderboard(
            APP_ID, LEADERBOARD_FORMAT_STR.format(daily)
        )
        if leaderboard.id == 0:
            self.handle_error("Leaderboard NOT FOUND", socket)
            return

        out = "<leaderboard>\n"
        out += f" <url>http://mossranking.com/xml/{leaderboard.id}.xml</url>\n"
        out += f" <lbid>{leaderboard.id}</lbid>\n"
        out += f" <name>{leaderboard.name}</name>\n"
        out += f" <display_name>{leaderboard.name}</display_name>\n"
        out += f" <entries>{leaderboard.entry_count}</entries>\n"
        out += " <sortmethod>2</sortmethod>\n"
        out += " <displaytype>1</displaytype>\n"
        out += "</leaderboard>\n"

        socket.sendall(out.encode("utf-8"))

    def handle_fetch(self, daily, socket):
        LOG.info("Received FETCH for %s", daily)

        leaderboard = self.steam_client.get_leaderboard(
            APP_ID, LEADERBOARD_FORMAT_STR.format(daily)
        )
        if leaderboard.id == 0:
            self.handle_error("Leaderboard NOT FOUND", socket)
            return

        out = "<response>\n"
        out += f"<appID>{APP_ID}</appID>\n"
        out += f"<leaderboardID>{leaderboard.id}</leaderboardID>\n"
        out += f"<name>{leaderboard.name}</name>\n"
        out += f"<entryStart>0</entryStart>\n"
        out += f"<entryEnd>{leaderboard.entry_count}</entryEnd>\n"
        out += f"<resultCount>{leaderboard.entry_count}</resultCount>\n"
        out += " <entries>\n"

        for entry in leaderboard:
            details = struct.unpack(b"<LL", entry.details)

            out += "  <entry>\n"
            out += f"   <steamid>{entry.steam_id_user}</steamid>\n"
            out += f"   <score>{entry.score}</score>\n"
            out += f"   <rank>{entry.global_rank}</rank>\n"
            out += "   <ugcid>-1</ugcid>\n"
            out += "   <details>{:02x}000000{:02x}000000</details>\n".format(
                details[0], details[1]
            )
            out += "  </entry>\n"

        out += " </entries>\n"
        out += "</response>\n"

        socket.sendall(out.encode("utf-8"))

    def handle_error(self, message, socket):
        out = "<response>\n"
        out += " <status>0</status>\n"
        out += f" <message>{message}</message>\n"
        out += "</response>\n"

        socket.sendall(out.encode("utf-8"))

    def __call__(self, socket, address):
        LOG.info("New connection from %s:%s" % address)

        rfileobj = socket.makefile(mode="rb")
        line = rfileobj.readline()
        if not line:
            LOG.info("client disconnected")
            rfileobj.close()
            return

        parts = line.strip().split()
        if len(parts) != 2 or parts[0] not in self.VALID_COMMANDS:
            print(parts)
            LOG.info("Invalid request")
            rfileobj.close()
            return

        cmd, daily = parts
        daily = daily.decode("utf-8")
        if cmd == b"FETCH":
            self.handle_fetch(daily, socket)
        elif cmd == b"INFO":
            self.handle_info(daily, socket)

        rfileobj.close()


if __name__ == "__main__":
    server = StreamServer(("0.0.0.0", 16000), RequestHandler(steam_client=client))
    print("Starting server on port 16000")
    server.start()

    try:
        result = client.login(username=USERNAME, password=PASSWORD)

        if result != EResult.OK:
            LOG.info("Failed to login: %s" % repr(result))
            raise SystemExit

        client.run_forever()
    except KeyboardInterrupt:
        if client.connected:
            LOG.info("Logout")
            client.logout()
            server.stop()
