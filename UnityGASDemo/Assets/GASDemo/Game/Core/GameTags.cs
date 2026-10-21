using GAS;

namespace GASDemo
{
    /// <summary>
    /// Native gameplay tags (UE_DEFINE_GAMEPLAY_TAG). Code refers to these fields; assets may use the same strings.
    /// The hierarchy is the design: cancelling "Ability" cancels everything, blocking "Ability.Skill" blocks all skills.
    /// </summary>
    public static class GameTags
    {
        static GameplayTag T(string name) => GameplayTag.Request(name);

        // Abilities -------------------------------------------------------------------------------------------
        public static readonly GameplayTag Ability = T("Ability");
        public static readonly GameplayTag Ability_Attack = T("Ability.Attack");
        public static readonly GameplayTag Ability_Attack_Basic = T("Ability.Attack.Basic");
        public static readonly GameplayTag Ability_Skill = T("Ability.Skill");
        public static readonly GameplayTag Ability_Skill_MysticShot = T("Ability.Skill.MysticShot");
        public static readonly GameplayTag Ability_Skill_FrostField = T("Ability.Skill.FrostField");
        public static readonly GameplayTag Ability_Movement_Tumble = T("Ability.Movement.Tumble");
        public static readonly GameplayTag Ability_Ultimate_Shockwave = T("Ability.Ultimate.Shockwave");
        public static readonly GameplayTag Ability_Passive = T("Ability.Passive");
        public static readonly GameplayTag Ability_Enemy_Attack = T("Ability.Enemy.Attack");

        // Cooldowns (granted by the shared cooldown effect via each ability's CooldownTags) -------------------
        public static readonly GameplayTag Cooldown = T("Cooldown");
        public static readonly GameplayTag Cooldown_MysticShot = T("Cooldown.Skill.MysticShot");
        public static readonly GameplayTag Cooldown_FrostField = T("Cooldown.Skill.FrostField");
        public static readonly GameplayTag Cooldown_Tumble = T("Cooldown.Movement.Tumble");
        public static readonly GameplayTag Cooldown_Shockwave = T("Cooldown.Ultimate.Shockwave");
        public static readonly GameplayTag Cooldown_Enemy = T("Cooldown.Enemy");

        // States ----------------------------------------------------------------------------------------------
        public static readonly GameplayTag State_Dead = T("State.Dead");
        public static readonly GameplayTag State_Attacking = T("State.Attacking");
        public static readonly GameplayTag State_Casting = T("State.Casting");
        public static readonly GameplayTag State_Dashing = T("State.Dashing");
        public static readonly GameplayTag State_Invulnerable = T("State.Invulnerable");
        public static readonly GameplayTag State_Empowered = T("State.Buff.Empowered");
        public static readonly GameplayTag State_Debuff = T("State.Debuff");
        public static readonly GameplayTag State_Debuff_Stun = T("State.Debuff.Stun");
        public static readonly GameplayTag State_Debuff_Slow = T("State.Debuff.Slow");
        public static readonly GameplayTag State_Debuff_Burn = T("State.Debuff.Burn");

        // Gameplay events -------------------------------------------------------------------------------------
        public static readonly GameplayTag Event_Hit = T("Event.Hit");
        public static readonly GameplayTag Event_Hit_Basic = T("Event.Hit.Basic");
        public static readonly GameplayTag Event_Hit_MysticShot = T("Event.Hit.MysticShot");
        public static readonly GameplayTag Event_Hit_Ability = T("Event.Hit.Ability");
        public static readonly GameplayTag Event_Damaged = T("Event.Damaged");
        public static readonly GameplayTag Event_Kill = T("Event.Kill");

        // SetByCaller data keys -------------------------------------------------------------------------------
        public static readonly GameplayTag Data_Damage = T("Data.Damage");
        public static readonly GameplayTag Data_ADRatio = T("Data.Ratio.AD");
        public static readonly GameplayTag Data_APRatio = T("Data.Ratio.AP");
        public static readonly GameplayTag Data_Cost = T("Data.Cost");
        public static readonly GameplayTag Data_Heal = T("Data.Heal");
        public static readonly GameplayTag Data_Mana = T("Data.Mana");

        // Effect descriptors ----------------------------------------------------------------------------------
        public static readonly GameplayTag Effect_Damage = T("Effect.Damage");
        public static readonly GameplayTag Effect_Boon = T("Effect.Boon");

        // Cues ------------------------------------------------------------------------------------------------
        public static readonly GameplayTag Cue_Hit_Physical = T("GameplayCue.Hit.Physical");
        public static readonly GameplayTag Cue_Hit_Magic = T("GameplayCue.Hit.Magic");
        public static readonly GameplayTag Cue_Hit_Burn = T("GameplayCue.Hit.Burn");
        public static readonly GameplayTag Cue_Heal = T("GameplayCue.Heal");
        public static readonly GameplayTag Cue_Status_Stun = T("GameplayCue.Status.Stun");
        public static readonly GameplayTag Cue_Status_Slow = T("GameplayCue.Status.Slow");
        public static readonly GameplayTag Cue_Status_Burn = T("GameplayCue.Status.Burn");
        public static readonly GameplayTag Cue_Status_Empowered = T("GameplayCue.Status.Empowered");
        public static readonly GameplayTag Cue_Shockwave = T("GameplayCue.Shockwave");
    }

    /// <summary>Input slots abilities are bound to (EAbilityInputID).</summary>
    public enum AbilityInputID
    {
        None = -1,
        Basic = 0,      // LMB
        Secondary = 1,  // RMB
        Skill = 2,      // Q
        Dash = 3,       // Space
        Ultimate = 4,   // R
    }

    public enum Team { Heroes, Monsters }
}
