using System.Collections.Generic;
using GAS;
using UnityEngine;

namespace GASDemo
{
    public enum EnemyKind { Grunt, Brute, Caster }

    /// <summary>
    /// All gameplay data for the demo, authored in code so the project runs with zero assets.
    /// Every object here is a ScriptableObject built exactly as a designer would fill it in the inspector;
    /// the "Tools/GAS Demo/Export Content As Assets" menu saves them to disk to continue editing that way.
    /// </summary>
    public class DemoContent
    {
        // ---- Shared effects -------------------------------------------------------------------------------
        public GameplayEffect GE_Damage_Physical, GE_Damage_Magic, GE_Burn, GE_Slow, GE_Stun, GE_Empowered;
        public GameplayEffect GE_Cost, GE_Cooldown, GE_RestoreMana, GE_Heal, GE_Regen, GE_EnemyScaling;
        public GameplayEffect GE_Boon_Might, GE_Boon_Sorcery, GE_Boon_Haste, GE_Boon_Vitality, GE_Boon_Swiftness, GE_Boon_Armor;

        // ---- Hero abilities -------------------------------------------------------------------------------
        public GA_MeleeCombo GA_Basic;
        public GA_FireProjectile GA_MysticShot;
        public GA_GroundZone GA_FrostField;
        public GA_Dash GA_Tumble;
        public GA_Shockwave GA_Shockwave;
        public GA_Passive_ArcaneEcho GA_ArcaneEcho;
        public GA_Passive_Ignite GA_Ignite;
        public GA_Passive_Bloodthirst GA_Bloodthirst;

        // ---- Enemy abilities ------------------------------------------------------------------------------
        public GA_TelegraphedStrike GA_GruntSwipe, GA_BruteSlam;
        public GA_FireProjectile GA_CasterBolt;

        public readonly List<ScriptableObject> AllAssets = new List<ScriptableObject>();

        public static readonly CharacterStats HeroStats = new CharacterStats
        {
            MaxHealth = 650f, MaxMana = 300f, AttackPower = 30f, AbilityPower = 40f, Armor = 20f, MoveSpeed = 6.5f,
        };

        static GameplayTagContainer Tags(params GameplayTag[] tags) => GameplayTagContainer.Of(tags);

        T New<T>(string name) where T : ScriptableObject
        {
            var so = ScriptableObject.CreateInstance<T>();
            so.name = name;
            AllAssets.Add(so);
            return so;
        }

        public static DemoContent Build()
        {
            var c = new DemoContent();
            c.BuildEffects();
            c.BuildHeroAbilities();
            c.BuildEnemyAbilities();
            return c;
        }

        // =====================================================================================================
        void BuildEffects()
        {
            var cannotBeHit = new GameplayTagRequirements { IgnoreTags = Tags(GameTags.State_Invulnerable, GameTags.State_Dead) };

            GE_Damage_Physical = New<GameplayEffect>("GE_Damage_Physical");
            GE_Damage_Physical.Executions.Add(new DamageExecution());
            GE_Damage_Physical.AssetTags = Tags(GameTags.Effect_Damage);
            GE_Damage_Physical.ApplicationTagRequirements = cannotBeHit;
            GE_Damage_Physical.GameplayCues = Tags(GameTags.Cue_Hit_Physical);

            GE_Damage_Magic = New<GameplayEffect>("GE_Damage_Magic");
            GE_Damage_Magic.Executions.Add(new DamageExecution { ArmorEffectiveness = 0.5f });
            GE_Damage_Magic.AssetTags = Tags(GameTags.Effect_Damage);
            GE_Damage_Magic.ApplicationTagRequirements = cannotBeHit;
            GE_Damage_Magic.GameplayCues = Tags(GameTags.Cue_Hit_Magic);

            GE_Burn = New<GameplayEffect>("GE_Burn");
            GE_Burn.DurationPolicy = EffectDurationPolicy.HasDuration;
            GE_Burn.DurationMagnitude = GameplayEffectModifierMagnitude.Scalable(3f);
            GE_Burn.Period = 0.5f;
            GE_Burn.ExecutePeriodicEffectOnApplication = false;
            GE_Burn.Executions.Add(new DamageExecution { ArmorEffectiveness = 0f });
            GE_Burn.AssetTags = Tags(GameTags.Effect_Damage);
            GE_Burn.GrantedTags = Tags(GameTags.State_Debuff_Burn);
            GE_Burn.ApplicationTagRequirements = cannotBeHit;
            GE_Burn.StackingType = EffectStackingType.AggregateBySource;
            GE_Burn.StackLimitCount = 4;
            GE_Burn.GameplayCues = Tags(GameTags.Cue_Status_Burn);

            GE_Slow = New<GameplayEffect>("GE_Slow");
            GE_Slow.DurationPolicy = EffectDurationPolicy.HasDuration;
            GE_Slow.DurationMagnitude = GameplayEffectModifierMagnitude.Scalable(0.75f);
            GE_Slow.Modifiers.Add(new GameplayModifierInfo(CharacterAttributeSet.MoveSpeed, ModifierOp.Multiply, GameplayEffectModifierMagnitude.Scalable(0.5f)));
            GE_Slow.GrantedTags = Tags(GameTags.State_Debuff_Slow);
            GE_Slow.ApplicationTagRequirements = cannotBeHit;
            GE_Slow.StackingType = EffectStackingType.AggregateByTarget;
            GE_Slow.StackLimitCount = 1;
            GE_Slow.GameplayCues = Tags(GameTags.Cue_Status_Slow);

            GE_Stun = New<GameplayEffect>("GE_Stun");
            GE_Stun.DurationPolicy = EffectDurationPolicy.HasDuration;
            GE_Stun.DurationMagnitude = GameplayEffectModifierMagnitude.Scalable(1f);
            GE_Stun.GrantedTags = Tags(GameTags.State_Debuff_Stun);
            GE_Stun.ApplicationTagRequirements = cannotBeHit;
            GE_Stun.GameplayCues = Tags(GameTags.Cue_Status_Stun);

            GE_Empowered = New<GameplayEffect>("GE_Empowered");
            GE_Empowered.DurationPolicy = EffectDurationPolicy.HasDuration;
            GE_Empowered.DurationMagnitude = GameplayEffectModifierMagnitude.Scalable(4f);
            GE_Empowered.GrantedTags = Tags(GameTags.State_Empowered);
            GE_Empowered.StackingType = EffectStackingType.AggregateByTarget;
            GE_Empowered.GameplayCues = Tags(GameTags.Cue_Status_Empowered);

            GE_Cost = New<GameplayEffect>("GE_Cost_Mana");
            GE_Cost.Modifiers.Add(new GameplayModifierInfo(CharacterAttributeSet.Mana, ModifierOp.Add, GameplayEffectModifierMagnitude.SetByCaller(GameTags.Data_Cost)));

            // One cooldown effect for every ability: duration and tags come from the ability (SetDuration + CooldownTags).
            GE_Cooldown = New<GameplayEffect>("GE_Cooldown");
            GE_Cooldown.DurationPolicy = EffectDurationPolicy.HasDuration;

            GE_RestoreMana = New<GameplayEffect>("GE_RestoreMana");
            GE_RestoreMana.Modifiers.Add(new GameplayModifierInfo(CharacterAttributeSet.Mana, ModifierOp.Add, GameplayEffectModifierMagnitude.SetByCaller(GameTags.Data_Mana)));

            GE_Heal = New<GameplayEffect>("GE_Heal");
            GE_Heal.Modifiers.Add(new GameplayModifierInfo(CharacterAttributeSet.Health, ModifierOp.Add, GameplayEffectModifierMagnitude.SetByCaller(GameTags.Data_Heal)));
            GE_Heal.GameplayCues = Tags(GameTags.Cue_Heal);

            GE_Regen = New<GameplayEffect>("GE_Regen");
            GE_Regen.DurationPolicy = EffectDurationPolicy.Infinite;
            GE_Regen.Period = 0.5f;
            GE_Regen.ExecutePeriodicEffectOnApplication = false;
            GE_Regen.Modifiers.Add(new GameplayModifierInfo(CharacterAttributeSet.Health, ModifierOp.Add, GameplayEffectModifierMagnitude.Scalable(2f)));
            GE_Regen.Modifiers.Add(new GameplayModifierInfo(CharacterAttributeSet.Mana, ModifierOp.Add, GameplayEffectModifierMagnitude.Scalable(5f)));

            // Applied to every enemy at level = wave number: +18% health and +10% attack per wave beyond the first.
            GE_EnemyScaling = New<GameplayEffect>("GE_EnemyScaling");
            GE_EnemyScaling.DurationPolicy = EffectDurationPolicy.Infinite;
            GE_EnemyScaling.Modifiers.Add(new GameplayModifierInfo(CharacterAttributeSet.MaxHealth, ModifierOp.Multiply, GameplayEffectModifierMagnitude.Scalable(1f, 0.18f)));
            GE_EnemyScaling.Modifiers.Add(new GameplayModifierInfo(CharacterAttributeSet.AttackPower, ModifierOp.Multiply, GameplayEffectModifierMagnitude.Scalable(1f, 0.10f)));

            GE_Boon_Might = Boon("GE_Boon_Might", CharacterAttributeSet.AttackPower, ModifierOp.Add, 12f);
            GE_Boon_Sorcery = Boon("GE_Boon_Sorcery", CharacterAttributeSet.AbilityPower, ModifierOp.Add, 30f);
            GE_Boon_Haste = Boon("GE_Boon_Haste", CharacterAttributeSet.AbilityHaste, ModifierOp.Add, 25f);
            GE_Boon_Vitality = Boon("GE_Boon_Vitality", CharacterAttributeSet.MaxHealth, ModifierOp.Add, 150f);
            GE_Boon_Swiftness = Boon("GE_Boon_Swiftness", CharacterAttributeSet.MoveSpeed, ModifierOp.Multiply, 1.12f);
            GE_Boon_Armor = Boon("GE_Boon_Armor", CharacterAttributeSet.Armor, ModifierOp.Add, 20f);
        }

        GameplayEffect Boon(string name, GameplayAttribute attribute, ModifierOp op, float value)
        {
            var ge = New<GameplayEffect>(name);
            ge.DurationPolicy = EffectDurationPolicy.Infinite;
            ge.Modifiers.Add(new GameplayModifierInfo(attribute, op, GameplayEffectModifierMagnitude.Scalable(value)));
            ge.AssetTags = Tags(GameTags.Effect_Boon);
            return ge;
        }

        /// <summary>Settings every active ability shares: cost/cooldown effects and "can't act while dead or stunned".</summary>
        void ConfigureActive(DemoGameplayAbility a, GameplayTag abilityTag, GameplayTag cooldownTag, float cooldown, float cooldownPerLevel = 0f)
        {
            a.AbilityTags = Tags(abilityTag);
            a.ActivationBlockedTags = Tags(GameTags.State_Dead, GameTags.State_Debuff_Stun);
            a.CostEffect = GE_Cost;
            if (cooldownTag.IsValid)
            {
                a.CooldownEffect = GE_Cooldown;
                a.CooldownDuration = new ScalableFloat(cooldown, cooldownPerLevel);
                a.CooldownTags = Tags(cooldownTag);
            }
        }

        void ConfigurePassive(DemoGameplayAbility a, GameplayTag triggerEvent)
        {
            a.AbilityTags = Tags(GameTags.Ability_Passive);
            a.ActivationBlockedTags = Tags(GameTags.State_Dead);
            a.AbilityTriggers.Add(new AbilityTriggerData(triggerEvent, AbilityTriggerSource.GameplayEvent));
        }

        // =====================================================================================================
        void BuildHeroAbilities()
        {
            GA_Basic = New<GA_MeleeCombo>("GA_Basic_Combo");
            ConfigureActive(GA_Basic, GameTags.Ability_Attack_Basic, default, 0f);
            GA_Basic.DisplayName = "Blade Combo";
            GA_Basic.Description = "Three-hit combo. The third hit is a wide finisher with knockback. Empowered attacks deal bonus magic damage.";
            GA_Basic.InputID = AbilityInputID.Basic;
            GA_Basic.ActivationOwnedTags = Tags(GameTags.State_Attacking);
            GA_Basic.ActivationBlockedTags.AddTag(GameTags.State_Dashing);
            GA_Basic.ActivationBlockedTags.AddTag(GameTags.State_Casting);
            GA_Basic.DamageEffect = GE_Damage_Physical;

            GA_MysticShot = New<GA_FireProjectile>("GA_MysticShot");
            ConfigureActive(GA_MysticShot, GameTags.Ability_Skill_MysticShot, GameTags.Cooldown_MysticShot, 4f, -0.25f);
            GA_MysticShot.DisplayName = "Mystic Shot";
            GA_MysticShot.Description = "Fire a bolt. On hit, all cooldowns are reduced by 1.5s and you restore mana (Arcane Echo).";
            GA_MysticShot.InputID = AbilityInputID.Secondary;
            GA_MysticShot.ManaCost = new ScalableFloat(25f);
            GA_MysticShot.CancelAbilitiesWithTag = Tags(GameTags.Ability_Attack);
            GA_MysticShot.ActivationBlockedTags.AddTag(GameTags.State_Dashing);
            GA_MysticShot.ActivationBlockedTags.AddTag(GameTags.State_Casting);
            GA_MysticShot.DamageEffect = GE_Damage_Magic;
            GA_MysticShot.HitEventTag = GameTags.Event_Hit_MysticShot;

            GA_FrostField = New<GA_GroundZone>("GA_FrostField");
            ConfigureActive(GA_FrostField, GameTags.Ability_Skill_FrostField, GameTags.Cooldown_FrostField, 9f, -0.5f);
            GA_FrostField.DisplayName = "Frost Field";
            GA_FrostField.Description = "Freeze the ground at the cursor: enemies inside take magic damage over time and are slowed by 50%.";
            GA_FrostField.InputID = AbilityInputID.Skill;
            GA_FrostField.ManaCost = new ScalableFloat(45f);
            GA_FrostField.ActivationBlockedTags.AddTag(GameTags.State_Dashing);
            GA_FrostField.ActivationBlockedTags.AddTag(GameTags.State_Casting);
            GA_FrostField.DamageEffect = GE_Damage_Magic;
            GA_FrostField.SlowEffect = GE_Slow;

            GA_Tumble = New<GA_Dash>("GA_Tumble");
            ConfigureActive(GA_Tumble, GameTags.Ability_Movement_Tumble, GameTags.Cooldown_Tumble, 3.5f, -0.3f);
            GA_Tumble.DisplayName = "Tumble";
            GA_Tumble.Description = "Dash in your movement direction, invulnerable while dashing. Empowers your next basic attack.";
            GA_Tumble.InputID = AbilityInputID.Dash;
            GA_Tumble.ActivationOwnedTags = Tags(GameTags.State_Dashing, GameTags.State_Invulnerable);
            GA_Tumble.CancelAbilitiesWithTag = Tags(GameTags.Ability_Attack);
            GA_Tumble.ActivationBlockedTags.AddTag(GameTags.State_Casting);
            GA_Tumble.EmpowerEffect = GE_Empowered;

            GA_Shockwave = New<GA_Shockwave>("GA_Shockwave");
            ConfigureActive(GA_Shockwave, GameTags.Ability_Ultimate_Shockwave, GameTags.Cooldown_Shockwave, 20f, -2f);
            GA_Shockwave.DisplayName = "Shockwave";
            GA_Shockwave.Description = "Channel, then slam the ground: heavy damage, knockback, and a stun that grows with rank.";
            GA_Shockwave.InputID = AbilityInputID.Ultimate;
            GA_Shockwave.ManaCost = new ScalableFloat(80f);
            GA_Shockwave.ActivationOwnedTags = Tags(GameTags.State_Casting);
            GA_Shockwave.CancelAbilitiesWithTag = Tags(GameTags.Ability_Attack);
            GA_Shockwave.ActivationBlockedTags.AddTag(GameTags.State_Dashing);
            GA_Shockwave.DamageEffect = GE_Damage_Magic;
            GA_Shockwave.StunEffect = GE_Stun;
            GA_Shockwave.MaxLevel = 3;

            GA_ArcaneEcho = New<GA_Passive_ArcaneEcho>("GA_Passive_ArcaneEcho");
            ConfigurePassive(GA_ArcaneEcho, GameTags.Event_Hit_MysticShot);
            GA_ArcaneEcho.DisplayName = "Arcane Echo";
            GA_ArcaneEcho.RestoreManaEffect = GE_RestoreMana;

            GA_Ignite = New<GA_Passive_Ignite>("GA_Passive_Ignite");
            ConfigurePassive(GA_Ignite, GameTags.Event_Hit);
            GA_Ignite.DisplayName = "Ignite";
            GA_Ignite.BurnEffect = GE_Burn;

            GA_Bloodthirst = New<GA_Passive_Bloodthirst>("GA_Passive_Bloodthirst");
            ConfigurePassive(GA_Bloodthirst, GameTags.Event_Kill);
            GA_Bloodthirst.DisplayName = "Bloodthirst";
            GA_Bloodthirst.HealEffect = GE_Heal;
        }

        void BuildEnemyAbilities()
        {
            GA_GruntSwipe = New<GA_TelegraphedStrike>("GA_Grunt_Swipe");
            ConfigureActive(GA_GruntSwipe, GameTags.Ability_Enemy_Attack, GameTags.Cooldown_Enemy, 1.3f);
            GA_GruntSwipe.ActivationOwnedTags = Tags(GameTags.State_Casting);
            GA_GruntSwipe.DamageEffect = GE_Damage_Physical;
            GA_GruntSwipe.Shape = GA_TelegraphedStrike.StrikeShape.Cone;
            GA_GruntSwipe.Range = 2.1f;
            GA_GruntSwipe.Angle = 100f;
            GA_GruntSwipe.WindUp = 0.55f;
            GA_GruntSwipe.Recovery = 0.45f;
            GA_GruntSwipe.BaseDamage = new ScalableFloat(14f);

            GA_BruteSlam = New<GA_TelegraphedStrike>("GA_Brute_Slam");
            ConfigureActive(GA_BruteSlam, GameTags.Ability_Enemy_Attack, GameTags.Cooldown_Enemy, 3.5f);
            GA_BruteSlam.ActivationOwnedTags = Tags(GameTags.State_Casting);
            GA_BruteSlam.DamageEffect = GE_Damage_Physical;
            GA_BruteSlam.Shape = GA_TelegraphedStrike.StrikeShape.Circle;
            GA_BruteSlam.Range = 3.3f;
            GA_BruteSlam.WindUp = 1.0f;
            GA_BruteSlam.Recovery = 0.6f;
            GA_BruteSlam.BaseDamage = new ScalableFloat(30f);
            GA_BruteSlam.Knockback = 11f;
            GA_BruteSlam.TelegraphColor = new Color(1f, 0.45f, 0.1f, 0.3f);

            GA_CasterBolt = New<GA_FireProjectile>("GA_Caster_Bolt");
            ConfigureActive(GA_CasterBolt, GameTags.Ability_Enemy_Attack, GameTags.Cooldown_Enemy, 2.6f);
            GA_CasterBolt.ActivationOwnedTags = Tags(GameTags.State_Casting);
            GA_CasterBolt.DamageEffect = GE_Damage_Magic;
            GA_CasterBolt.BaseDamage = new ScalableFloat(16f);
            GA_CasterBolt.ADRatio = 1f;
            GA_CasterBolt.APRatio = 0f;
            GA_CasterBolt.Speed = 12f;
            GA_CasterBolt.Range = 14f;
            GA_CasterBolt.Radius = 0.4f;
            GA_CasterBolt.VisualSize = 0.5f;
            GA_CasterBolt.WindUp = 0.7f;
            GA_CasterBolt.Color = new Color(0.85f, 0.35f, 1f);
        }

        // =====================================================================================================
        public CharacterBase SpawnHero(Vector3 position)
        {
            var hero = CreateCharacter("Hero", position, Team.Heroes, new Color(0.25f, 0.55f, 1f), 1f, true);
            hero.DisplayName = "Hero";
            hero.DestroyOnDeath = false;
            hero.InitializeStats(HeroStats);
            hero.gameObject.AddComponent<HeroInput>();

            var asc = hero.AbilitySystem;
            foreach (var ability in new DemoGameplayAbility[] { GA_Basic, GA_MysticShot, GA_FrostField, GA_Tumble, GA_Shockwave, GA_ArcaneEcho })
                asc.GiveAbility(ability, 1, (int)ability.InputID);
            asc.ApplyGameplayEffectToSelf(GE_Regen);
            return hero;
        }

        public CharacterBase SpawnEnemy(EnemyKind kind, Vector3 position, int waveLevel)
        {
            CharacterBase enemy;
            EnemyBrain brain;
            switch (kind)
            {
                case EnemyKind.Brute:
                    enemy = CreateCharacter("Brute", position, Team.Monsters, new Color(0.6f, 0.15f, 0.12f), 1.5f, false);
                    enemy.InitializeStats(new CharacterStats { MaxHealth = 340f, AttackPower = 8f, Armor = 35f, MoveSpeed = 2.7f });
                    enemy.AbilitySystem.GiveAbility(GA_BruteSlam);
                    brain = enemy.gameObject.AddComponent<EnemyBrain>();
                    brain.AttackRange = 2.4f;
                    break;

                case EnemyKind.Caster:
                    enemy = CreateCharacter("Caster", position, Team.Monsters, new Color(0.55f, 0.25f, 0.75f), 0.9f, false);
                    enemy.InitializeStats(new CharacterStats { MaxHealth = 75f, AttackPower = 6f, Armor = 5f, MoveSpeed = 3.3f });
                    enemy.AbilitySystem.GiveAbility(GA_CasterBolt);
                    brain = enemy.gameObject.AddComponent<EnemyBrain>();
                    brain.AttackRange = 10f;
                    brain.KeepAwayDistance = 6f;
                    break;

                default:
                    enemy = CreateCharacter("Grunt", position, Team.Monsters, new Color(0.85f, 0.32f, 0.25f), 1f, false);
                    enemy.InitializeStats(new CharacterStats { MaxHealth = 95f, AttackPower = 6f, Armor = 12f, MoveSpeed = 3.8f });
                    enemy.AbilitySystem.GiveAbility(GA_GruntSwipe);
                    brain = enemy.gameObject.AddComponent<EnemyBrain>();
                    brain.AttackRange = 1.6f;
                    break;
            }

            enemy.DisplayName = kind.ToString();
            if (waveLevel > 1)
            {
                enemy.AbilitySystem.ApplyGameplayEffectToSelf(GE_EnemyScaling, waveLevel);
                enemy.RefillVitals();
            }
            return enemy;
        }

        static CharacterBase CreateCharacter(string name, Vector3 position, Team team, Color color, float scale, bool isHero)
        {
            var go = new GameObject(name);
            go.transform.SetParent(Fx.Root, false);
            go.transform.position = position;

            var character = go.AddComponent<CharacterBase>();
            character.Team = team;
            character.BodyColor = color;
            character.Radius = 0.5f * scale;
            character.HealthBarHeight = 2.3f * scale;

            var body = new GameObject("Body").transform;
            body.SetParent(go.transform, false);
            Fx.Primitive(PrimitiveType.Capsule, Vector3.zero, new Vector3(0.9f, 1f, 0.9f) * scale, color, false, body).transform.localPosition = Vector3.up * scale;
            // A "nose" so facing is readable from the top-down camera.
            Fx.Primitive(PrimitiveType.Cube, Vector3.zero, new Vector3(0.3f, 0.25f, 0.5f) * scale, color, false, body).transform.localPosition = new Vector3(0f, 1.35f, 0.4f) * scale;
            if (isHero)
            {
                var blade = Fx.Primitive(PrimitiveType.Cube, Vector3.zero, new Vector3(0.12f, 0.12f, 1.3f), color, false, body);
                blade.transform.localPosition = new Vector3(0.55f, 1f, 0.55f);
            }
            character.SetBody(body);
            return character;
        }
    }
}
