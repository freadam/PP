using System.Collections.Generic;
using System.Linq;
using System.Text;
using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// IMGUI HUD — zero setup, and it reads everything straight from the ASC: attributes for bars, cooldown effects
    /// for the ability bar, owned tags and active effects for the F1 debug panel (UE's "showdebug abilitysystem").
    /// </summary>
    public class DemoHUD : MonoBehaviour
    {
        public CharacterBase Hero;
        public WaveDirector Director;
        public DemoContent Content;
        public System.Action RestartRequested;

        static readonly (AbilityInputID id, string key)[] Slots =
        {
            (AbilityInputID.Basic, "LMB"),
            (AbilityInputID.Secondary, "RMB"),
            (AbilityInputID.Skill, "Q"),
            (AbilityInputID.Dash, "SPACE"),
            (AbilityInputID.Ultimate, "R"),
        };

        bool showDebug;
        GUIStyle label, center, small, big, numbers;
        Camera cam;

        void Update()
        {
            FloatingText.Tick(Time.unscaledDeltaTime);
            if (InputReader.Down(DemoButton.ToggleDebug)) showDebug = !showDebug;
            if (Director != null && (Director.CurrentPhase == WaveDirector.Phase.GameOver || Director.CurrentPhase == WaveDirector.Phase.Victory)
                && InputReader.Down(DemoButton.Confirm))
                RestartRequested?.Invoke();
        }

        void EnsureStyles()
        {
            if (label != null) return;
            label = new GUIStyle(GUI.skin.label) { fontSize = 14, richText = true };
            label.normal.textColor = Color.white;
            center = new GUIStyle(label) { alignment = TextAnchor.MiddleCenter };
            small = new GUIStyle(label) { fontSize = 11 };
            big = new GUIStyle(center) { fontSize = 34, fontStyle = FontStyle.Bold };
            numbers = new GUIStyle(center) { fontSize = 18, fontStyle = FontStyle.Bold };
        }

        void OnGUI()
        {
            if (Hero == null) return;
            EnsureStyles();
            if (cam == null) cam = Camera.main;

            DrawWorldBars();
            DrawFloatingText();
            DrawHeroPanel();
            DrawRunInfo();
            if (showDebug) DrawDebugPanel();
            DrawPhaseOverlays();
        }

        // ------------------------------------------------------------------------------------------------ world

        void DrawWorldBars()
        {
            if (cam == null) return;
            foreach (var c in Targeting.All)
            {
                if (!c.IsAlive) continue;
                var screen = cam.WorldToScreenPoint(c.transform.position + Vector3.up * c.HealthBarHeight);
                if (screen.z < 0f) continue;
                float w = c == Hero ? 70f : 50f;
                var rect = new Rect(screen.x - w * 0.5f, Screen.height - screen.y - 10f, w, 7f);
                float hp = c.Attributes.GetHealth() / Mathf.Max(1f, c.Attributes.GetMaxHealth());
                Bar(rect, hp, c.Team == Team.Heroes ? new Color(0.3f, 0.9f, 0.4f) : new Color(0.95f, 0.25f, 0.2f));

                if (c.HasTag(GameTags.State_Debuff_Stun))
                    GUI.Label(new Rect(rect.x - 20f, rect.y - 20f, w + 40f, 18f), "<b>STUNNED</b>", center);
                else if (c.HasTag(GameTags.State_Casting) && c != Hero)
                    GUI.Label(new Rect(rect.x - 20f, rect.y - 20f, w + 40f, 18f), "<color=#ff9a6a>!</color>", center);
            }
        }

        void DrawFloatingText()
        {
            if (cam == null) return;
            foreach (var e in FloatingText.Entries)
            {
                var screen = cam.WorldToScreenPoint(e.WorldPosition);
                if (screen.z < 0f) continue;
                float t = e.Age / e.Lifetime;
                var pos = new Vector2(screen.x + e.Jitter.x, Screen.height - screen.y - 30f * t + e.Jitter.y);
                var old = GUI.color;
                GUI.color = e.Color.WithAlpha(1f - t * t);
                GUI.Label(new Rect(pos.x - 60f, pos.y - 12f, 120f, 24f), e.Text, numbers);
                GUI.color = old;
            }
        }

        // ------------------------------------------------------------------------------------------------ hero

        void DrawHeroPanel()
        {
            var asc = Hero.AbilitySystem;
            const float slot = 74f, gap = 8f;
            float totalW = Slots.Length * slot + (Slots.Length - 1) * gap;
            float x0 = (Screen.width - totalW) * 0.5f;
            float y0 = Screen.height - slot - 22f;

            float hp = Hero.Attributes.GetHealth(), maxHp = Hero.Attributes.GetMaxHealth();
            float mp = Hero.Attributes.GetMana(), maxMp = Hero.Attributes.GetMaxMana();
            var hpRect = new Rect(x0, y0 - 44f, totalW, 16f);
            var mpRect = new Rect(x0, y0 - 24f, totalW, 12f);
            Bar(hpRect, hp / maxHp, new Color(0.3f, 0.85f, 0.4f));
            Bar(mpRect, mp / maxMp, new Color(0.3f, 0.55f, 1f));
            GUI.Label(hpRect, $"{Mathf.CeilToInt(hp)} / {Mathf.CeilToInt(maxHp)}", new GUIStyle(center) { fontSize = 11 });
            GUI.Label(mpRect, $"{Mathf.FloorToInt(mp)} / {Mathf.CeilToInt(maxMp)}", new GUIStyle(center) { fontSize = 10 });

            for (int i = 0; i < Slots.Length; i++)
            {
                var rect = new Rect(x0 + i * (slot + gap), y0, slot, slot);
                var spec = asc.FindAbilitySpecFromInputID((int)Slots[i].id);
                DrawAbilitySlot(rect, spec, Slots[i].key);
            }
        }

        void DrawAbilitySlot(Rect rect, GameplayAbilitySpec spec, string key)
        {
            Fill(rect, new Color(0f, 0f, 0f, 0.65f));
            if (spec == null) return;

            var ability = spec.Ability as DemoGameplayAbility;
            bool onCooldown = spec.Ability.GetCooldownTimeRemainingAndDuration(out float remaining, out float duration);
            bool affordable = spec.Ability.CheckCost(null);

            Fill(new Rect(rect.x + 2, rect.y + 2, rect.width - 4, rect.height - 4),
                spec.IsActive ? new Color(0.9f, 0.8f, 0.3f, 0.5f) : affordable ? new Color(0.2f, 0.3f, 0.45f, 0.9f) : new Color(0.15f, 0.15f, 0.35f, 0.9f));

            if (onCooldown && duration > 0f)
            {
                float frac = remaining / duration;
                Fill(new Rect(rect.x + 2, rect.y + 2 + (rect.height - 4) * (1f - frac), rect.width - 4, (rect.height - 4) * frac), new Color(0f, 0f, 0f, 0.7f));
                GUI.Label(rect, $"<b>{remaining:0.0}</b>", new GUIStyle(center) { fontSize = 20 });
            }

            string title = ability != null ? ability.DisplayName : spec.Ability.name;
            GUI.Label(new Rect(rect.x + 3, rect.y + 2, rect.width - 6, 30), title, new GUIStyle(small) { wordWrap = true });
            GUI.Label(new Rect(rect.x, rect.yMax - 18, rect.width - 4, 16), $"<b>{key}</b>", new GUIStyle(small) { alignment = TextAnchor.LowerRight });
            if (ability != null)
            {
                GUI.Label(new Rect(rect.x + 4, rect.yMax - 18, 40, 16), new string('•', spec.Level), new GUIStyle(small) { normal = { textColor = new Color(1f, 0.85f, 0.3f) } });
                float cost = ability.GetManaCost();
                if (cost > 0f)
                    GUI.Label(new Rect(rect.x, rect.y + 30, rect.width - 4, 16), $"<color=#7fb0ff>{cost:0}</color>", new GUIStyle(small) { alignment = TextAnchor.UpperRight });
            }
        }

        void DrawRunInfo()
        {
            var sb = new StringBuilder();
            if (Director != null)
            {
                sb.AppendLine($"<b>Wave {Mathf.Max(1, Director.Wave)} / {Director.FinalWave}</b>    Enemies: {Director.AliveEnemies}    Kills: {Director.Kills}");
                if (Director.CurrentPhase == WaveDirector.Phase.Intermission)
                    sb.AppendLine($"Next wave in {Director.PhaseTimer:0.0}s");
            }
            sb.AppendLine("<size=11>WASD move · Mouse aim · LMB combo · RMB Mystic Shot · Q Frost Field · SPACE Tumble · R Shockwave · F1 GAS debug</size>");
            GUI.Label(new Rect(14, 10, 900, 60), sb.ToString(), label);
        }

        // ------------------------------------------------------------------------------------------------ debug

        void DrawDebugPanel()
        {
            var asc = Hero.AbilitySystem;
            var sb = new StringBuilder();
            sb.AppendLine("<b>ATTRIBUTES</b>  (current / base)");
            foreach (var set in asc.AttributeSets)
                foreach (var attr in set.Attributes)
                    sb.AppendLine($"  {attr.Name,-14} {asc.GetNumericAttribute(attr),8:0.0} / {asc.GetNumericAttributeBase(attr):0.0}");

            sb.AppendLine().AppendLine("<b>OWNED TAGS</b>");
            foreach (var tag in asc.OwnedTags.ExplicitTags.OrderBy(t => t.Name))
                sb.AppendLine($"  {tag}  x{asc.OwnedTags.GetExplicitTagCount(tag)}");

            sb.AppendLine().AppendLine("<b>ACTIVE EFFECTS</b>");
            foreach (var e in asc.ActiveEffects.Effects)
            {
                string time = e.IsInfinite ? "∞" : $"{e.TimeRemaining:0.0}s";
                string stacks = e.StackCount > 1 ? $" x{e.StackCount}" : "";
                string granted = e.Spec.GetAllGrantedTags().IsEmpty ? "" : " " + e.Spec.GetAllGrantedTags();
                sb.AppendLine($"  {e.Def.name}{stacks}  {time}{granted}");
            }

            sb.AppendLine().AppendLine("<b>ABILITIES</b>");
            foreach (var spec in asc.ActivatableAbilities)
                sb.AppendLine($"  {(spec.IsActive ? "<color=#ffd34d>●</color>" : "○")} {spec.Ability.name}  lv{spec.Level}  {spec.Ability.AbilityTags}");

            var rect = new Rect(Screen.width - 430, 10, 420, Screen.height - 140);
            Fill(rect, new Color(0f, 0f, 0f, 0.7f));
            GUI.Label(new Rect(rect.x + 8, rect.y + 6, rect.width - 16, rect.height - 12), sb.ToString(), new GUIStyle(small) { fontSize = 12 });
        }

        // ------------------------------------------------------------------------------------------------ phases

        void DrawPhaseOverlays()
        {
            if (Director == null) return;
            switch (Director.CurrentPhase)
            {
                case WaveDirector.Phase.ChoosingBoon:
                    DrawBoonChoice();
                    break;
                case WaveDirector.Phase.GameOver:
                    Overlay("YOU DIED", $"Reached wave {Director.Wave} · {Director.Kills} kills\nPress ENTER to restart");
                    break;
                case WaveDirector.Phase.Victory:
                    Overlay("VICTORY", $"All {Director.FinalWave} waves cleared · {Director.Kills} kills\nPress ENTER to play again");
                    break;
            }
        }

        void DrawBoonChoice()
        {
            Fill(new Rect(0, 0, Screen.width, Screen.height), new Color(0f, 0f, 0f, 0.55f));
            GUI.Label(new Rect(0, Screen.height * 0.18f, Screen.width, 50), $"Wave {Director.Wave} cleared — choose a boon", big);

            var choices = Director.BoonChoices;
            const float w = 240f, h = 150f, gap = 24f;
            float total = choices.Count * w + (choices.Count - 1) * gap;
            float x0 = (Screen.width - total) * 0.5f, y0 = Screen.height * 0.36f;
            for (int i = 0; i < choices.Count; i++)
            {
                var rect = new Rect(x0 + i * (w + gap), y0, w, h);
                if (GUI.Button(rect, ""))
                {
                    Director.ChooseBoon(i);
                    return;
                }
                GUI.Label(new Rect(rect.x, rect.y + 14, w, 30), $"<b>[{i + 1}]  {choices[i].Title}</b>", new GUIStyle(center) { fontSize = 18 });
                GUI.Label(new Rect(rect.x + 14, rect.y + 52, w - 28, h - 60), choices[i].Description, new GUIStyle(center) { wordWrap = true, alignment = TextAnchor.UpperCenter });
            }
        }

        void Overlay(string title, string subtitle)
        {
            Fill(new Rect(0, 0, Screen.width, Screen.height), new Color(0f, 0f, 0f, 0.6f));
            GUI.Label(new Rect(0, Screen.height * 0.35f, Screen.width, 60), title, big);
            GUI.Label(new Rect(0, Screen.height * 0.35f + 60, Screen.width, 60), subtitle, center);
        }

        // ------------------------------------------------------------------------------------------------ helpers

        static void Fill(Rect r, Color c)
        {
            var old = GUI.color;
            GUI.color = c;
            GUI.DrawTexture(r, Texture2D.whiteTexture);
            GUI.color = old;
        }

        static void Bar(Rect r, float fraction, Color color)
        {
            Fill(r, new Color(0f, 0f, 0f, 0.7f));
            Fill(new Rect(r.x + 1, r.y + 1, (r.width - 2) * Mathf.Clamp01(fraction), r.height - 2), color);
        }
    }
}
