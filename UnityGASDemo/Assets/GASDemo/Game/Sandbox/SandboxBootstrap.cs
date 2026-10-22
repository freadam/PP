using System.Collections.Generic;
using System.Text;
using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Ability test range: the hero plus stationary capsule dummies laid out to exercise each ability
    /// (a line for the combo and skillshots, a cluster for AoE, an armored one, a fragile one for on-kill passives).
    /// No waves, no enemies attacking — just hit things and read the numbers.
    /// Use Tools > GAS Demo > Create Ability Sandbox Scene, or put this on an empty GameObject.
    /// </summary>
    public class SandboxBootstrap : MonoBehaviour
    {
        struct DummySlot
        {
            public Vector3 Position;
            public string Label;
            public float MaxHealth, Armor;
            public Color Color;
            public CharacterBase Instance;
            public float RespawnAt;
        }

        DemoContent content;
        CharacterBase hero;
        readonly List<DummySlot> dummies = new List<DummySlot>();
        bool freeCasts;
        GUIStyle labelStyle, panelStyle;
        Camera cam;

        void Start()
        {
            Time.timeScale = 1f;
            Targeting.Clear();
            FloatingText.Entries.Clear();
            DemoCues.RegisterAll();

            content = DemoContent.Build();
            Fx.Root = new GameObject("GASDemo_Sandbox").transform;
            DemoBootstrap.BuildArena();
            DemoBootstrap.EnsureLight();

            hero = content.SpawnHero(new Vector3(0f, 0f, -6f));
            hero.DestroyOnDeath = false;
            DemoBootstrap.SetupCamera(hero.transform);

            var tan = new Color(0.8f, 0.68f, 0.45f);
            // Line in front of the hero: combo cone, Mystic Shot, Tumble-empowered hits.
            AddDummy(new Vector3(-2.5f, 0f, 2f), "Line A", 3000f, 0f, tan);
            AddDummy(new Vector3(0f, 0f, 2f), "Line B", 3000f, 0f, tan);
            AddDummy(new Vector3(2.5f, 0f, 2f), "Line C", 3000f, 0f, tan);
            // Tight cluster: Frost Field ticks and slows, Shockwave stun + knockback (dummies are anchored).
            float ring = 1.6f;
            for (int i = 0; i < 5; i++)
            {
                float a = i / 5f * Mathf.PI * 2f;
                AddDummy(new Vector3(9f + Mathf.Cos(a) * ring, 0f, 3f + Mathf.Sin(a) * ring), "Pack " + (i + 1), 3000f, 0f, new Color(0.7f, 0.6f, 0.4f));
            }
            // Armor comparison: same hit, ~60% less physical damage (magic ignores half of armor).
            AddDummy(new Vector3(-9f, 0f, 3f), "Armored (150)", 3000f, 150f, new Color(0.55f, 0.58f, 0.65f));
            // Dies in a couple of hits and respawns: Event.Kill / Bloodthirst.
            AddDummy(new Vector3(-5f, 0f, -3f), "Fragile", 60f, 0f, new Color(0.9f, 0.45f, 0.45f));
            // Near the edge of Mystic Shot's 13m range from the start position.
            AddDummy(new Vector3(0f, 0f, 6.5f), "Far", 3000f, 0f, tan);

            var hud = Fx.Root.gameObject.AddComponent<DemoHUD>();
            hud.Hero = hero;
            hud.Content = content;
        }

        void AddDummy(Vector3 position, string label, float maxHealth, float armor, Color color)
        {
            var slot = new DummySlot { Position = position, Label = label, MaxHealth = maxHealth, Armor = armor, Color = color };
            slot.Instance = Spawn(slot);
            dummies.Add(slot);
        }

        CharacterBase Spawn(DummySlot slot)
        {
            var d = content.SpawnDummy(slot.Position, slot.Label, slot.MaxHealth, slot.Armor, slot.Color);
            d.FaceTowards(hero.transform.position);
            return d;
        }

        void Update()
        {
            RespawnDeadDummies();
            HandleCheats();
            if (freeCasts) ResetHeroResources();
        }

        void RespawnDeadDummies()
        {
            for (int i = 0; i < dummies.Count; i++)
            {
                var slot = dummies[i];
                bool gone = slot.Instance == null || !slot.Instance.IsAlive;
                if (!gone) continue;
                if (slot.RespawnAt <= 0f) slot.RespawnAt = Time.time + 1.5f;
                else if (Time.time >= slot.RespawnAt)
                {
                    if (slot.Instance != null) Destroy(slot.Instance.gameObject);
                    slot.Instance = Spawn(slot);
                    slot.RespawnAt = 0f;
                    Fx.Burst(slot.Position + Vector3.up, slot.Color.WithAlpha(0.6f), 0.5f, 2f, 0.25f);
                }
                dummies[i] = slot;
            }
        }

        void HandleCheats()
        {
            if (Pressed(KeyCode.F2)) ResetHeroResources();
            if (Pressed(KeyCode.F3)) freeCasts = !freeCasts;
            if (Pressed(KeyCode.F4)) TogglePassive(content.GA_Ignite);
            if (Pressed(KeyCode.F5)) TogglePassive(content.GA_Bloodthirst);
            if (Pressed(KeyCode.F6)) RankUpAll();
            if (Pressed(KeyCode.F7)) StunAllDummies();
        }

        void ResetHeroResources()
        {
            hero.RefillVitals();
            hero.AbilitySystem.ActiveEffects.ModifyTimeRemaining(GameplayTagContainer.Of(GameTags.Cooldown), -999f);
        }

        void TogglePassive(GameplayAbility passive)
        {
            var asc = hero.AbilitySystem;
            var spec = asc.FindAbilitySpecFromDefinition(passive);
            if (spec != null) asc.ClearAbility(spec.Handle);
            else asc.GiveAbility(passive);
            FloatingText.Spawn(hero.transform.position + Vector3.up * 2.8f, (spec == null ? "+ " : "- ") + passive.name.Replace("GA_Passive_", ""), new Color(1f, 0.85f, 0.3f));
        }

        void RankUpAll()
        {
            foreach (var spec in hero.AbilitySystem.ActivatableAbilities)
            {
                if (!(spec.Definition is DemoGameplayAbility a) || a.InputID == AbilityInputID.None) continue;
                spec.Level = spec.Level >= a.MaxLevel ? 1 : spec.Level + 1;
            }
            FloatingText.Spawn(hero.transform.position + Vector3.up * 2.8f, "Ranks changed", new Color(1f, 0.85f, 0.3f));
        }

        void StunAllDummies()
        {
            var stun = hero.AbilitySystem.MakeOutgoingSpec(content.GE_Stun);
            stun.SetDuration(2f);
            foreach (var slot in dummies)
                if (slot.Instance != null && slot.Instance.IsAlive)
                    hero.AbilitySystem.ApplyGameplayEffectSpecToTarget(stun, slot.Instance.AbilitySystem);
        }

        static bool Pressed(KeyCode key)
        {
#if ENABLE_INPUT_SYSTEM && GASDEMO_INPUT_SYSTEM
            var k = UnityEngine.InputSystem.Keyboard.current;
            if (k == null) return false;
            switch (key)
            {
                case KeyCode.F2: return k.f2Key.wasPressedThisFrame;
                case KeyCode.F3: return k.f3Key.wasPressedThisFrame;
                case KeyCode.F4: return k.f4Key.wasPressedThisFrame;
                case KeyCode.F5: return k.f5Key.wasPressedThisFrame;
                case KeyCode.F6: return k.f6Key.wasPressedThisFrame;
                case KeyCode.F7: return k.f7Key.wasPressedThisFrame;
            }
            return false;
#else
            return Input.GetKeyDown(key);
#endif
        }

        void OnGUI()
        {
            if (hero == null) return;
            if (labelStyle == null)
            {
                labelStyle = new GUIStyle(GUI.skin.label) { fontSize = 12, richText = true, alignment = TextAnchor.LowerCenter };
                labelStyle.normal.textColor = Color.white;
                panelStyle = new GUIStyle(GUI.skin.label) { fontSize = 13, richText = true };
                panelStyle.normal.textColor = Color.white;
            }
            if (cam == null) cam = Camera.main;

            if (cam != null)
            {
                foreach (var slot in dummies)
                {
                    if (slot.Instance == null || !slot.Instance.IsAlive) continue;
                    var meter = slot.Instance.GetComponent<TrainingDummy>();
                    var screen = cam.WorldToScreenPoint(slot.Instance.transform.position + Vector3.up * slot.Instance.HealthBarHeight);
                    if (screen.z < 0f) continue;
                    GUI.Label(new Rect(screen.x - 90f, Screen.height - screen.y - 70f, 180f, 56f), meter.Describe(), labelStyle);
                }
            }

            var asc = hero.AbilitySystem;
            var sb = new StringBuilder();
            sb.AppendLine("<b>ABILITY SANDBOX</b>");
            sb.AppendLine("F2  refill + reset cooldowns");
            sb.AppendLine($"F3  free casts: {(freeCasts ? "<color=#7dff8a>ON</color>" : "off")}");
            sb.AppendLine($"F4  Ignite passive: {OnOff(asc.FindAbilitySpecFromDefinition(content.GA_Ignite) != null)}");
            sb.AppendLine($"F5  Bloodthirst passive: {OnOff(asc.FindAbilitySpecFromDefinition(content.GA_Bloodthirst) != null)}");
            sb.AppendLine("F6  rank up all abilities (wraps)");
            sb.AppendLine("F7  stun all dummies (2s)");
            sb.AppendLine("F1  GAS debug panel");
            GUI.Label(new Rect(14, 64, 320, 160), sb.ToString(), panelStyle);
        }

        static string OnOff(bool on) => on ? "<color=#7dff8a>ON</color>" : "off";

        void OnDestroy()
        {
            if (Fx.Root != null) Destroy(Fx.Root.gameObject);
        }
    }
}
