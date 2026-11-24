"""Exercise an unmodified release APK through the Android accessibility UI."""
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

PACKAGE = "io.linguatutor"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "android-evidence"
EVIDENCE.mkdir(parents=True, exist_ok=True)
REMOTE_XML = "/sdcard/lingua-acceptance.xml"


def adb(*args, timeout=20):
    return subprocess.check_output(["adb", *args], timeout=timeout)


def shell(*args):
    return adb("shell", *args)


def dump():
    shell("rm", "-f", REMOTE_XML)
    shell("uiautomator", "dump", REMOTE_XML)
    data = adb("exec-out", "cat", REMOTE_XML)
    (EVIDENCE / "last-ui.xml").write_bytes(data)
    return ET.fromstring(data)


def find(*labels, timeout=60, scroll=False, editable=False):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            tree = dump()
            for node in tree.iter("node"):
                names = [node.get(k, "") for k in ("text", "content-desc", "resource-id")]
                if editable and node.get("class") != "android.widget.EditText":
                    continue
                if not any(label == name or name.endswith(":id/" + label)
                           for label in labels for name in names):
                    continue
                bounds = list(map(int, re.findall(r"\d+", node.get("bounds", ""))))
                if len(bounds) == 4 and bounds[2] > bounds[0] and bounds[3] > bounds[1]:
                    return node, bounds
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, ET.ParseError):
            pass
        if scroll:
            shell("input", "swipe", "850", "1450", "850", "650", "350")
        time.sleep(1)
    raise AssertionError(f"UI element not found: {labels}")


def tap(*labels, scroll=False, editable=False):
    _, (left, top, right, bottom) = find(*labels, scroll=scroll, editable=editable)
    shell("input", "tap", str((left + right) // 2), str((top + bottom) // 2))


def snapshot(name):
    (EVIDENCE / f"{name}.png").write_bytes(adb("exec-out", "screencap", "-p"))
    dump()
    (EVIDENCE / f"{name}.xml").write_bytes((EVIDENCE / "last-ui.xml").read_bytes())


def hide_keyboard():
    state = shell("dumpsys", "input_method").decode(errors="replace")
    if re.search(r"\b(?:mInputShown|mIsInputViewShown)=true\b", state):
        shell("input", "keyevent", "4")


def enter(label, placeholder, value):
    tap(label, placeholder, editable=True, scroll=True)
    shell("input", "text", value)
    hide_keyboard()


def launch():
    result = shell("am", "start", "-W", "-n", PACKAGE + "/.MainActivity").decode()
    if "Status: ok" not in result:
        raise AssertionError(result)
    find("Connect your OpenAI tunnel")
    tap("Continue locally", scroll=True)
    find("Choose a language to begin", "Due for review")


def export(label, filename):
    tap(label, scroll=True)
    tap("Save")
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            data = adb("exec-out", "cat", "/sdcard/Download/" + filename)
            if data:
                (EVIDENCE / filename).write_bytes(data)
                return data
        except subprocess.CalledProcessError:
            pass
        time.sleep(1)
    raise AssertionError(f"Export not saved: {filename}")


def main(tag):
    shell("logcat", "-c")
    try:
        launch()
        tap("Create or select a language profile")
        find("No profiles yet")
        enter("target-locale", "Search language or BCP 47 locale", "es-ES")
        enter("source-language", "Search explanation language", "en")
        tap("Create profile", scroll=True)
        find("es-ES")
        tap("es-ES")
        tap("Home", "Lingua Tutor home")
        find("Due for review")
        find("Next lesson")
        snapshot("dashboard")
        shell("am", "force-stop", PACKAGE)
        launch()
        find("Due for review")
        tap("Languages")
        find("es-ES")
        snapshot("persisted-profile")
        print("PASS: install, UI, profile write, dashboard read, process restart, persisted profile")

        # v0.1.10 has no navigable export entry in its WebView.
        if tag == "v0.1.10":
            print("NOT TESTED: export entry is absent in v0.1.10")
        else:
            import csv
            import io
            import json
            tap("Export data", scroll=True)
            find("Full JSON export")
            # Select Downloads in the system picker if necessary.
            data = json.loads(export("Full JSON export", "lingua-export.json"))
            assert data["schema_version"] == "1" and data["language_profile_id"]
            rows = list(csv.reader(io.StringIO(export("Cards CSV", "cards.csv").decode())))
            assert rows[0] == ["id", "intent", "status", "prompt", "answer",
                               "accepted_answers", "creation_reason"]
            print("PASS: JSON and CSV saved via Android document picker")

        crashes = shell("logcat", "-d", "-b", "crash").decode(errors="replace")
        if PACKAGE in crashes:
            raise AssertionError(crashes)
        print("PASS: no application crash in Logcat")
    finally:
        (EVIDENCE / "logcat.txt").write_bytes(shell("logcat", "-d"))
        (EVIDENCE / "last-screen.png").write_bytes(adb("exec-out", "screencap", "-p"))


if __name__ == "__main__":
    main(sys.argv[1])
