// Lives in Resources/ so Shader.Find also works in player builds, not just in the editor.
// Flat alpha-blended color for telegraphs, bursts and auras. Plain CG with no LightMode tag, so it renders in the
// built-in pipeline and in URP (SRPDefaultUnlit pass) alike. Tinted per renderer via MaterialPropertyBlock (_Color).
Shader "GASDemo/UnlitTransparent"
{
    Properties
    {
        _Color ("Color", Color) = (1, 1, 1, 0.5)
    }
    SubShader
    {
        Tags { "Queue" = "Transparent" "RenderType" = "Transparent" "IgnoreProjector" = "True" }
        Blend SrcAlpha OneMinusSrcAlpha
        ZWrite Off
        Cull Off

        Pass
        {
            CGPROGRAM
            #pragma vertex vert
            #pragma fragment frag
            #include "UnityCG.cginc"

            fixed4 _Color;

            float4 vert (float4 vertex : POSITION) : SV_POSITION
            {
                return UnityObjectToClipPos(vertex);
            }

            fixed4 frag () : SV_Target
            {
                return _Color;
            }
            ENDCG
        }
    }
}
