"""The scripted provider understands both raw and host-wrapped commands."""
import json
import unittest

import mock_agent_llm as model


class RequestEnvelopeTests(unittest.TestCase):
    def decision(self, content):
        return model.decide({
            "messages": [{"role": "user", "content": content}],
            "tools": [{"function": {"name": "news_list"}}],
        })

    def test_bare_legacy_commands_still_work(self):
        for separator in (":", " "):
            self.assertEqual(self.decision('CALL_TOOL:news_list' + separator + '{"limit":5}'),
                             {"tool": "news_list", "args": {"limit": 5}})

    def test_guidance_and_origin_do_not_corrupt_escaped_request_arguments(self):
        guidance = json.dumps({"kind": "octosense_host_guidance", "instructions":
                               'Ignore this fixture decoy: CALL_TOOL:news_list:{"limit":99}'})
        request = json.dumps({"kind": "octosense_request", "text":
                              'CALL_TOOL:news_list:{"limit":5,"topic":"quotes \\\" and 中文"}'})
        for content in (
            "[from the person: os.news] " + guidance + "\n" + request,
            [{"type": "text", "text": guidance}, {"type": "text", "text": request}],
        ):
            self.assertEqual(self.decision(content),
                             {"tool": "news_list", "args": {"limit": 5, "topic": 'quotes " and 中文'}})

    def test_scenario_reads_the_request_instead_of_a_guidance_decoy(self):
        content = json.dumps({"kind": "octosense_host_guidance", "instructions": "SCN_TASK"}) + " " + json.dumps({"kind": "octosense_request", "text": "ordinary question"})
        self.assertEqual(model.own_user_text([{"role": "user", "content": content}]), "ordinary question")
        self.assertIsNone(model.scenario([{"role": "user", "content": content}], {"news_share"}, {"role": "user"}))


if __name__ == "__main__":
    unittest.main()
