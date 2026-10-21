using UnityEngine;

namespace GASDemo
{
    /// <summary>Angled follow camera (Hades / Ravenswatch framing) with trauma-style shake.</summary>
    public class TopDownCamera : MonoBehaviour
    {
        public Transform Target;
        public Vector3 Offset = new Vector3(0f, 17f, -11f);
        public float FollowSharpness = 8f;

        static float shakeTime, shakeStrength;
        Vector3 smoothed;

        public static void Shake(float duration, float strength)
        {
            shakeTime = Mathf.Max(shakeTime, duration);
            shakeStrength = Mathf.Max(shakeStrength, strength);
        }

        void Start()
        {
            if (Target != null) smoothed = Target.position;
        }

        void LateUpdate()
        {
            if (Target != null)
                smoothed = Vector3.Lerp(smoothed, Target.position, 1f - Mathf.Exp(-FollowSharpness * Time.unscaledDeltaTime));

            var shake = Vector3.zero;
            if (shakeTime > 0f)
            {
                shakeTime -= Time.unscaledDeltaTime;
                shake = Random.insideUnitSphere * shakeStrength;
                if (shakeTime <= 0f) shakeStrength = 0f;
            }

            transform.position = smoothed + Offset + shake;
            transform.rotation = Quaternion.LookRotation(-Offset, Vector3.up);
        }
    }
}
