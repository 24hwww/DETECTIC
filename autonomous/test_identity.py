import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from autonomous.identity import (
    AssociationState,
    DeviceClass,
    DeviceIdentityEngine,
    EntityType,
    EvidenceType,
    MacType,
    Observation,
    AliasMap,
    classify_mac,
    combine_confidence,
    confidence_label,
    confidence_word,
    infer_device_class,
    is_generic_hostname,
    is_randomized,
    manufacturer,
    normalize_mac,
    oui,
    stable_fingerprint,
)
from autonomous.identity.model import Evidence

SECRET = b"test-secret-key-16b"


def _obs(**kw):
    base = dict(
        sensor_id="test-sensor",
        timestamp=1787660000,
        entity_type=EntityType.CONNECTED_CLIENT,
    )
    base.update(kw)
    return Observation(**base)


class TestMac(unittest.TestCase):
    def test_global_unicast(self):
        self.assertEqual(classify_mac("3C:6A:D2:5F:AB:C1"), MacType.GLOBAL_UNICAST)

    def test_invalid(self):
        self.assertEqual(classify_mac("not-a-mac"), MacType.INVALID)
        self.assertEqual(classify_mac(""), MacType.INVALID)
        self.assertEqual(classify_mac(None), MacType.INVALID)

    def test_locally_administered_randomized(self):
        # second-least-significant bit of first octet set => locally administered
        self.assertEqual(classify_mac("02:00:00:00:00:01"), MacType.LOCAL_RANDOMIZED)
        self.assertTrue(is_randomized("02:00:00:00:00:01"))

    def test_multicast_global(self):
        self.assertEqual(classify_mac("01:00:00:00:00:00"), MacType.GLOBAL_MULTICAST)

    def test_normalize(self):
        self.assertEqual(normalize_mac("3C:6A:D2:5F:AB:C1"), "3c6ad25fabc1")
        self.assertEqual(normalize_mac("3C-6A-D2-5F-AB-C1"), "3c6ad25fabc1")

    def test_oui(self):
        self.assertEqual(oui("3C:6A:D2:5F:AB:C1"), "3C6AD2")
        self.assertIsNone(oui("invalid"))


class TestOui(unittest.TestCase):
    def test_known_oui(self):
        # Motorola / Lenovo OUI present in curated DB
        self.assertIn(manufacturer("001D5E123456"), "Motorola")

    def test_apple_oui(self):
        self.assertEqual(manufacturer("0019E0123456"), "Apple")

    def test_unknown_oui(self):
        self.assertIsNone(manufacturer("FFFFFFFFFFFF".lower()))

    def test_locally_administered_no_manufacturer(self):
        # randomized MAC must NOT resolve a manufacturer
        self.assertIsNone(manufacturer("02:00:00:00:00:01"))

    def test_malformed(self):
        self.assertIsNone(manufacturer("zz:zz:zz:zz:zz:zz"))


class TestIdentity(unittest.TestCase):
    def test_hostname_plus_oui(self):
        obs = _obs(
            mac="001D5E123456",
            hostname="moto-g42",
            protocol="n",
            band="2.4GHz",
            association_state=AssociationState.ASSOCIATED,
        )
        eng = DeviceIdentityEngine()
        ident = eng.identify(obs, SECRET)
        self.assertEqual(ident.manufacturer, "Motorola")
        self.assertEqual(ident.device_class, DeviceClass.SMARTPHONE)
        self.assertEqual(ident.model_guess, "Moto G42")
        self.assertGreaterEqual(ident.confidence, 0.75)
        # privacy: raw mac never in identity dict
        self.assertNotIn("001D5E123456", str(ident.to_dict()))

    def test_oui_only(self):
        obs = _obs(mac="0019E0123456")
        ident = DeviceIdentityEngine().identify(obs, SECRET)
        self.assertEqual(ident.manufacturer, "Apple")
        self.assertEqual(ident.device_class, DeviceClass.UNKNOWN)
        self.assertIsNone(ident.model_guess)

    def test_hostname_only_unknown_mac(self):
        obs = _obs(mac="02:00:00:00:00:01", hostname="moto-g42")
        ident = DeviceIdentityEngine().identify(obs, SECRET)
        # randomized MAC => no manufacturer inferred
        self.assertIsNone(ident.manufacturer)
        # but hostname still drives class + model
        self.assertEqual(ident.device_class, DeviceClass.SMARTPHONE)
        self.assertEqual(ident.model_guess, "Moto G42")

    def test_randomized_mac_no_oui_inference(self):
        obs = _obs(mac="02:00:00:00:00:01")
        ident = DeviceIdentityEngine().identify(obs, SECRET)
        self.assertEqual(ident.mac_type, MacType.LOCAL_RANDOMIZED)
        self.assertIsNone(ident.manufacturer)
        self.assertLess(ident.confidence, 0.5)

    def test_unknown_device_low_confidence(self):
        obs = _obs(mac="FFFFFFFFFF00")
        ident = DeviceIdentityEngine().identify(obs, SECRET)
        self.assertEqual(ident.device_class, DeviceClass.UNKNOWN)
        self.assertIsNone(ident.manufacturer)
        self.assertLess(ident.confidence, 0.25)

    def test_amazon_smart_speaker(self):
        obs = _obs(mac="001B25123456", hostname="amazon-07a4dcc48")
        ident = DeviceIdentityEngine().identify(obs, SECRET)
        self.assertEqual(ident.manufacturer, "Amazon")
        self.assertEqual(ident.device_class, DeviceClass.SMART_SPEAKER)

    def test_conflicting_evidence_uses_hostname(self):
        # hostname says moto but OUI (hijacked/weird) — hostname wins for class
        obs = _obs(mac="0019E0123456", hostname="moto-g42")
        ident = DeviceIdentityEngine().identify(obs, SECRET)
        self.assertEqual(ident.device_class, DeviceClass.SMARTPHONE)
        # model guessed from hostname despite Apple OUI
        self.assertEqual(ident.model_guess, "Moto G42")


class TestConfidence(unittest.TestCase):
    def test_deterministic(self):
        ev = [
            Evidence(EvidenceType.OUI_MATCH, "x", 0.35, "oui"),
            Evidence(EvidenceType.HOSTNAME_MATCH, "y", 0.40, "hostname"),
        ]
        self.assertEqual(combine_confidence(ev), combine_confidence(list(ev)))

    def test_bounds(self):
        ev = [Evidence(EvidenceType.OUI_MATCH, "x", 1.0, "oui")] * 5
        self.assertLessEqual(combine_confidence(ev), 1.0)
        self.assertGreaterEqual(combine_confidence(ev), 0.0)

    def test_empty(self):
        self.assertEqual(combine_confidence([]), 0.0)

    def test_label(self):
        self.assertEqual(confidence_label(0.95), "very high")
        self.assertEqual(confidence_label(0.8), "high")
        self.assertEqual(confidence_label(0.6), "medium")
        self.assertEqual(confidence_label(0.3), "low")
        self.assertEqual(confidence_label(0.1), "very low")
        self.assertEqual(confidence_word(0.8), "Likely")
        self.assertEqual(confidence_word(0.1), "Unknown")


class TestPrivacy(unittest.TestCase):
    def test_raw_mac_not_in_payload(self):
        obs = _obs(mac="3C6AD25FABC1", hostname="moto-g42")
        ident = DeviceIdentityEngine().identify(obs, SECRET)
        payload = str(ident.to_dict())
        self.assertNotIn("3c6ad25fabc1", payload.lower())
        self.assertNotIn("3C6AD25FABC1", payload)

    def test_pseudonym_deterministic(self):
        obs1 = Observation(sensor_id="s", timestamp=1, mac="AA:BB:CC:DD:EE:FF")
        obs2 = Observation(sensor_id="s", timestamp=2, mac="AA:BB:CC:DD:EE:FF")
        e = DeviceIdentityEngine()
        self.assertEqual(e.identify(obs1, SECRET).pseudonym,
                         e.identify(obs2, SECRET).pseudonym)

    def test_pseudonym_sensor_scoped(self):
        obs = Observation(sensor_id="s", timestamp=1, mac="AA:BB:CC:DD:EE:FF")
        e1 = DeviceIdentityEngine().identify(obs, b"secret-A")
        e2 = DeviceIdentityEngine().identify(obs, b"secret-B")
        self.assertNotEqual(e1.pseudonym, e2.pseudonym)


class TestTemporal(unittest.TestCase):
    def test_first_seen_then_correlation(self):
        eng = DeviceIdentityEngine()
        obs = Observation(
            sensor_id="s", timestamp=100, mac="001D5E123456",
            association_state=AssociationState.ASSOCIATED,
        )
        i1 = eng.identify(obs, SECRET)
        self.assertEqual(eng.repos.get_temporal(i1.pseudonym).observation_count, 1)
        obs2 = Observation(
            sensor_id="s", timestamp=200, mac="001D5E123456",
            association_state=AssociationState.ASSOCIATED,
        )
        i2 = eng.identify(obs2, SECRET)
        self.assertEqual(i2.evidence[-1].type, EvidenceType.TEMPORAL_CORRELATION)
        self.assertEqual(eng.repos.get_temporal(i1.pseudonym).observation_count, 2)


class TestStableFingerprint(unittest.TestCase):
    def test_multiband_same_device_same_fingerprint(self):
        """A device on 2.4GHz and 5GHz (different MACs) must share fingerprint_id."""
        fp24 = stable_fingerprint(SECRET, "moto-g42", "Motorola", DeviceClass.SMARTPHONE,
                                  "aa:bb:cc:dd:ee:01")
        fp5 = stable_fingerprint(SECRET, "moto-g42", "Motorola", DeviceClass.SMARTPHONE,
                                 "aa:bb:cc:dd:ee:02")
        self.assertEqual(fp24.fingerprint_id, fp5.fingerprint_id)
        self.assertEqual(fp24.method, "hostname")
        self.assertGreaterEqual(fp24.confidence, 0.9)

    def test_mac_rotation_with_hostname_stays_stable(self):
        """Randomized MAC rotation on reconnect keeps the same fingerprint_id
        as long as the hostname is specific."""
        fp_a = stable_fingerprint(SECRET, "realme-9i", None, DeviceClass.SMARTPHONE,
                                  "02:11:22:33:44:55")
        fp_b = stable_fingerprint(SECRET, "realme-9i", None, DeviceClass.SMARTPHONE,
                                  "02:aa:bb:cc:dd:ee")
        self.assertEqual(fp_a.fingerprint_id, fp_b.fingerprint_id)

    def test_randomized_mac_no_hostname_changes(self):
        """Without a usable hostname, a rotated randomized MAC yields a new id."""
        fp_a = stable_fingerprint(SECRET, None, None, DeviceClass.UNKNOWN,
                                  "02:11:22:33:44:55")
        fp_b = stable_fingerprint(SECRET, None, None, DeviceClass.UNKNOWN,
                                  "02:aa:bb:cc:dd:ee")
        self.assertNotEqual(fp_a.fingerprint_id, fp_b.fingerprint_id)
        self.assertEqual(fp_a.method, "mac_randomized")
        self.assertLess(fp_a.confidence, 0.5)

    def test_global_mac_stable_without_hostname(self):
        """A non-randomized MAC (e.g. IoT) is a stable key by itself."""
        fp1 = stable_fingerprint(SECRET, None, None, DeviceClass.UNKNOWN,
                                 "00:19:e0:12:34:56")
        fp2 = stable_fingerprint(SECRET, None, None, DeviceClass.UNKNOWN,
                                 "00:19:e0:12:34:56")
        self.assertEqual(fp1.fingerprint_id, fp2.fingerprint_id)
        self.assertEqual(fp1.method, "mac")

    def test_generic_hostname_not_used_as_key(self):
        self.assertTrue(is_generic_hostname("iphone"))
        self.assertTrue(is_generic_hostname("android"))
        self.assertTrue(is_generic_hostname("Unknown"))
        self.assertTrue(is_generic_hostname(""))
        self.assertFalse(is_generic_hostname("moto-g42"))
        self.assertFalse(is_generic_hostname("amazon-07a4dcc48"))
        self.assertFalse(is_generic_hostname("soporte24hwww"))

    def test_hostname_and_mac_namespaces_disjoint(self):
        """A hostname-derived id can never collide with a mac-derived id."""
        fp_h = stable_fingerprint(SECRET, "moto-g42", "Motorola", DeviceClass.SMARTPHONE,
                                  "00:19:e0:12:34:56")
        fp_m = stable_fingerprint(SECRET, None, None, DeviceClass.UNKNOWN,
                                  "00:19:e0:12:34:56")
        self.assertNotEqual(fp_h.fingerprint_id, fp_m.fingerprint_id)

    def test_sensor_scoped(self):
        """Different secrets yield different fingerprint_ids (privacy scoping)."""
        fp1 = stable_fingerprint(b"secret-A", "moto-g42", "Motorola", DeviceClass.SMARTPHONE, "aa")
        fp2 = stable_fingerprint(b"secret-B", "moto-g42", "Motorola", DeviceClass.SMARTPHONE, "aa")
        self.assertNotEqual(fp1.fingerprint_id, fp2.fingerprint_id)


class TestAliasMap(unittest.TestCase):
    def test_register_and_resolve(self):
        m = AliasMap()
        m.register("fp1", "mac-a", ts=100, hostname="moto-g42", band="2.4GHz")
        m.register("fp1", "mac-b", ts=200, hostname="moto-g42", band="5GHz")
        self.assertEqual(m.fingerprint_of("mac-a"), "fp1")
        self.assertEqual(m.fingerprint_of("mac-b"), "fp1")
        self.assertEqual(sorted(m.aliases("fp1")), ["mac-a", "mac-b"])
        rec = m.get("fp1")
        self.assertEqual(rec.first_seen, 100)
        self.assertEqual(rec.last_seen, 200)
        self.assertIn("2.4GHz", rec.bands)
        self.assertIn("5GHz", rec.bands)

    def test_roundtrip(self):
        m = AliasMap()
        m.register("fp1", "mac-a", ts=10, hostname="h", band="2.4GHz")
        m.register("fp2", "mac-b", ts=20, hostname="h2", band="5GHz")
        m2 = AliasMap.from_dict(m.to_dict())
        self.assertEqual(sorted(m2.aliases("fp1")), ["mac-a"])
        self.assertEqual(m2.fingerprint_of("mac-b"), "fp2")


if __name__ == "__main__":
    unittest.main()
