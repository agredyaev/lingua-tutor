"""Exercise an unmodified release APK through the Android accessibility UI."""
import csv
import io
import json
import os
import shlex
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


def read_adb(*args, timeout=20, attempts=4):
    # Read-only commands can safely be repeated after transient ADB disconnects.
    for attempt in range(attempts):
        try:
            return adb(*args, timeout=timeout)
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
            if attempt == attempts - 1:
                raise
            print(f"Retrying read-only ADB command: {args}", flush=True)
            adb("wait-for-device", timeout=45)
            time.sleep(2)


def dump():
    shell("rm", "-f", REMOTE_XML)
    shell("uiautomator", "dump", REMOTE_XML)
    data = adb("exec-out", "cat", REMOTE_XML)
    (EVIDENCE / "last-ui.xml").write_bytes(data)
    return ET.fromstring(data)


def bounds(node):
    box = tuple(map(int, re.findall(r"\d+", node.get("bounds", ""))))
    if len(box) == 4 and box[2] > box[0] and box[3] > box[1]:
        return box
    return None


def matches(node, labels):
    return any(label == name or name.endswith(":id/" + label)
               for label in labels
               for name in (node.get(k, "") for k in ("text", "content-desc", "resource-id")))


def locate(tree, labels, *, field_label=None, clickable=False):
    nodes = [node for node in tree.iter("node") if bounds(node)]
    if field_label is None:
        return next((node for node in nodes if matches(node, labels)
                     and (not clickable or node.get("clickable") == "true")), None)

    fields = [node for node in nodes if node.get("class") == "android.widget.EditText"
              and node.get("enabled") == "true"]
    named = [node for node in fields if matches(node, (*labels, field_label))]
    if len(named) == 1:
        return named[0]

    # Android WebView may omit HTML id, placeholder and accessible name.
    # Pair the visible form label with the nearest input below in its column.
    anchors = [node for node in nodes if matches(node, (field_label,))]
    if len(anchors) != 1:
        return None
    left, _, right, bottom = bounds(anchors[0])
    candidates = []
    for node in fields:
        x1, y1, x2, _ = bounds(node)
        if y1 >= bottom and min(right, x2) > max(left, x1):
            candidates.append((y1 - bottom, node))
    candidates.sort(key=lambda candidate: candidate[0])
    if candidates and (len(candidates) == 1 or candidates[0][0] < candidates[1][0]):
        return candidates[0][1]
    return None


def find(*labels, timeout=60, scroll=False, field_label=None, clickable=False):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            tree = dump()
            node = locate(tree, labels, field_label=field_label, clickable=clickable)
            if node is not None:
                return node, bounds(node)
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, ET.ParseError):
            pass
        if scroll:
            try:
                shell("input", "swipe", "850", "1450", "850", "650", "350")
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                pass
        time.sleep(1)
    raise AssertionError(f"UI element not found: {labels}")


def tap(*labels, scroll=False, field_label=None, clickable=False):
    _, (left, top, right, bottom) = find(
        *labels, scroll=scroll, field_label=field_label, clickable=clickable)
    shell("input", "tap", str((left + right) // 2), str((top + bottom) // 2))


def find_profile(locale="es-ES", language="en", timeout=60):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            tree = dump()
            for node in tree.iter("node"):
                if node.get("class") != "android.widget.Button" or not bounds(node):
                    continue
                names = " ".join(n.get(k, "") for n in node.iter("node")
                                 for k in ("text", "content-desc"))
                if locale.casefold() in names.casefold().split() and f"{language} explanations" in names:
                    return node
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, ET.ParseError):
            pass
        time.sleep(1)
    raise AssertionError(f"Profile row not found: {locale}, {language}")


def snapshot(name):
    (EVIDENCE / f"{name}.png").write_bytes(adb("exec-out", "screencap", "-p"))
    dump()
    (EVIDENCE / f"{name}.xml").write_bytes((EVIDENCE / "last-ui.xml").read_bytes())


def hide_keyboard():
    state = read_adb("shell", "dumpsys", "input_method").decode(errors="replace")
    if re.search(r"\b(?:mInputShown|mIsInputViewShown)=true\b", state):
        shell("input", "keyevent", "4")


def enter(field_label, label, placeholder, value):
    tap(label, placeholder, field_label=field_label, scroll=True)
    adb("shell", "input text " + shlex.quote(value.replace(" ", "%s")), timeout=90)
    # Check the actual input value before proceeding, not just ADB exit status.
    # The app's persisted result is the definitive value assertion.
    # WebView does not consistently expose the entered value while IME is open.
    hide_keyboard()
    # Blur the input and close suggestions before resolving the next field.
    tap("Add a language", scroll=True)


def launch():
    result = shell("am", "start", "-W", "-n", PACKAGE + "/.MainActivity").decode()
    if "Status: ok" not in result:
        raise AssertionError(result)
    node, _ = find("Connect your OpenAI tunnel", "Choose a language to begin",
                   "Due for review", "Pixel Launcher isn't responding", timeout=120)
    if matches(node, ("Pixel Launcher isn't responding",)):
        # Explicitly distinguish an unrelated system launcher ANR from an app ANR.
        snapshot("pixel-launcher-anr")
        tap("Close app", clickable=True)
        result = shell("am", "start", "-W", "-n", PACKAGE + "/.MainActivity").decode()
        if "Status: ok" not in result:
            raise AssertionError(result)
        node, _ = find("Connect your OpenAI tunnel", "Choose a language to begin",
                       "Due for review", timeout=120)
    if matches(node, ("Connect your OpenAI tunnel",)):
        tap("Continue locally", scroll=True, clickable=True)
        find("Choose a language to begin", "Due for review")


def export(label, filename):
    tap(label, scroll=True)
    tap("Save", "SAVE", "button1", clickable=True)
    find(label)
    path = shlex.quote("/sdcard/Download/" + filename)
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            data = adb("shell", "-T", f"test -s {path} && cat {path}")
            if data:
                (EVIDENCE / filename).write_bytes(data)
                return data
        except subprocess.CalledProcessError:
            pass
        time.sleep(1)
    raise AssertionError(f"Export not saved: {filename}")


def select_profile(locale, language):
    node = find_profile(locale, language)
    left, top, right, bottom = bounds(node)
    shell("input", "tap", str((left + right) // 2), str((top + bottom) // 2))


def create_profile(locale, language):
    enter("Target locale", "target-locale", "Search language or BCP 47 locale", locale)
    enter("Explanation language", "source-language", "Search explanation language", language)
    tap("Create profile", scroll=True, clickable=True)
    find_profile(locale, language)
    print(f"PASS: created {locale}/{language} through UI", flush=True)


def navigate(label):
    tap(label, clickable=True)


def empty_state_checks(prefix):
    navigate("Review")
    find("Session done — reviewed 0 cards.")
    snapshot(prefix + "-review")
    navigate("Library")
    find("No cards in this view")
    snapshot(prefix + "-library")
    navigate("Progress")
    find("No reviews yet.")
    find("Due next 7 days")
    snapshot(prefix + "-progress")
    navigate("History")
    find("No lessons yet")
    snapshot(prefix + "-history")
    print(f"PASS: {prefix} review, library, progress and history return empty learner data", flush=True)


def main(tag):
    print(f"Checking Android local workflow for {tag}", flush=True)
    shell("logcat", "-c")
    try:
        launch()
        tap("Create or select a language profile")
        find("No profiles yet")
        snapshot("profile-form")
        create_profile("es-ES", "en")
        select_profile("es-ES", "en")
        navigate("Home")
        find("Due for review")
        find("Next lesson")
        snapshot("dashboard")
        empty_state_checks("spanish")
        navigate("Languages")
        find("Active profiles")
        create_profile("fr-FR", "en")
        select_profile("fr-FR", "en")
        empty_state_checks("french")
        navigate("Languages")
        find_profile("es-ES", "en")
        find_profile("fr-FR", "en")
        select_profile("es-ES", "en")
        shell("am", "force-stop", PACKAGE)
        launch()
        find("Due for review")
        tap("Languages", clickable=True)
        find_profile("es-ES", "en")
        find_profile("fr-FR", "en")
        select_profile("es-ES", "en")
        snapshot("persisted-profile")
        print("PASS: two profiles persisted after process restart", flush=True)
        # Loopback runtime must remain usable without internet.
        shell("svc", "wifi", "disable")
        shell("svc", "data", "disable")
        try:
            navigate("Home")
            find("Due for review")
            empty_state_checks("offline")
        finally:
            shell("svc", "wifi", "enable")
            shell("svc", "data", "enable")
        navigate("Languages")
        select_profile("es-ES", "en")

        navigate("Home")
        tap("Export data", scroll=True, clickable=True)
        find("Full JSON export")
        data = json.loads(export("Full JSON export", "lingua-export.json"))
        assert data["schema_version"] == "1" and data["language_profile_id"]
        assert data["lessons"] == [] and data["cards"] == []
        rows = list(csv.reader(io.StringIO(export("Cards CSV", "cards.csv").decode())))
        assert rows == [["id", "intent", "status", "prompt", "answer",
                         "accepted_answers", "creation_reason"]]
        print("PASS: JSON and CSV saved via Android document picker", flush=True)

        crashes = read_adb("logcat", "-b", "crash", "-d").decode(errors="replace")
        (EVIDENCE / "crash-logcat.txt").write_text(crashes)
        if PACKAGE in crashes:
            raise AssertionError(crashes)
        print("PASS: no application crash in Logcat")
        print("BLOCKED: live MCP lesson requires a connected test learner", flush=True)
        print("SCOPE: local release acceptance only; live MCP lesson and physical ARM64 NOT VERIFIED")
    finally:
        # Capture independently; a diagnostic failure must not mask the test failure.
        for name, args in (
            ("logcat.txt", ("logcat", "-d", "-t", "2000")),
            ("last-screen.png", ("exec-out", "screencap", "-p")),
        ):
            try:
                (EVIDENCE / name).write_bytes(adb(*args, timeout=45))
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
                print(f"Evidence capture failed for {name}: {exc}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
