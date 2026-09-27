#!/usr/bin/env python3

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import set_steam_wrap


class SteamWrapTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.home = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _write_config(self, user: str, appid: str) -> Path:
        path = self.home / ".local/share/Steam/userdata" / user / "config" / "localconfig.vdf"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f'"{appid}"\n\t\t\t\t\t{{\n\t\t\t\t\t\t"LastPlayed"\t\t"1"\n\t\t\t\t\t}}\n')
        return path

    def test_apply_returns_none_when_appid_missing(self) -> None:
        path = self._write_config("111", "999")
        result = set_steam_wrap.apply(path, "204030", "wrap")
        self.assertIsNone(result)

    def test_main_skips_users_without_app_and_fails_if_none_match(self) -> None:
        self._write_config("111", "999")
        self._write_config("222", "888")
        argv = [
            "set_steam_wrap.py",
            "--appid",
            "204030",
            "--user",
            "alice",
            "--name",
            "Fable",
            "--home",
            str(self.home),
        ]
        with mock.patch.object(set_steam_wrap, "steam_running", return_value=False):
            with mock.patch.object(sys, "argv", argv):
                with self.assertRaises(SystemExit) as raised:
                    set_steam_wrap.main()
        self.assertIn("no apps block for 204030", str(raised.exception))

    def test_main_applies_only_matching_user_config(self) -> None:
        self._write_config("111", "999")
        matching = self._write_config("222", "204030")
        argv = [
            "set_steam_wrap.py",
            "--appid",
            "204030",
            "--user",
            "alice",
            "--name",
            "Fable",
            "--home",
            str(self.home),
        ]
        with mock.patch.object(set_steam_wrap, "steam_running", return_value=False):
            with mock.patch.object(sys, "argv", argv):
                code = set_steam_wrap.main()
        self.assertEqual(code, 0)
        self.assertIn("ludusavi-lutris-wrap", matching.read_text())
        other = self.home / ".local/share/Steam/userdata/111/config/localconfig.vdf"
        self.assertNotIn("ludusavi-lutris-wrap", other.read_text())


if __name__ == "__main__":
    raise SystemExit(unittest.main(verbosity=2))
