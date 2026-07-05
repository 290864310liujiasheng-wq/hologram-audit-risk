import json
import unittest
from pathlib import Path


class AuthPaymentLiveSamplesTest(unittest.TestCase):
    def test_pending_semantics(self) -> None:
        samples_path = Path(__file__).resolve().parents[1] / "docs" / "auth-payment-live-samples.json"
        samples = json.loads(samples_path.read_text(encoding="utf-8"))

        auth_exchange_pro = samples["auth_exchange_pro"]["entitlement"]
        payment_query_pending = samples["payment_query_pending"]["entitlement"]

        self.assertEqual(auth_exchange_pro["plan"], "pro_personal_monthly")
        self.assertNotIn(
            "payment_pending",
            auth_exchange_pro,
            "Successful Pro exchange sample must not be marked payment_pending.",
        )

        self.assertEqual(payment_query_pending["plan"], "core_free")
        self.assertTrue(
            payment_query_pending["payment_pending"],
            "Pending payment sample must explicitly set payment_pending=true.",
        )


if __name__ == "__main__":
    unittest.main()
