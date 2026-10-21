/*{
  "DESCRIPTION": "(lostjared/shaders: frac_shader_dmd4_glass)",
  "CREDIT": "Jared Bruni (lostjared/shaders, MIT)",
  "ISFVSN": "2",
  "CATEGORIES": [
    "lostjared"
  ],
  "INPUTS": [
    {
      "NAME": "inputImage",
      "TYPE": "image"
    },
    {
      "NAME": "audioFFT",
      "TYPE": "audioFFT"
    }
  ]
}*/

float lj_spec(float x) { return IMG_NORM_PIXEL(audioFFT, vec2(clamp(x, 0.0, 1.0), 0.5)).r; }
vec4 lj_spec4(float x) { return vec4(lj_spec(x)); }
float lj_band(float a, float b) { return (lj_spec(a) + lj_spec(mix(a, b, 0.33)) + lj_spec(mix(a, b, 0.66)) + lj_spec(b)) * 0.25; }
#define amp_low lj_band(0.0, 0.08)
#define amp_mid lj_band(0.08, 0.35)
#define amp_high lj_band(0.35, 0.9)
#define amp_rms lj_band(0.0, 0.5)
#define amp_smooth lj_band(0.0, 0.5)
#define amp_peak max(lj_band(0.0, 0.1), lj_band(0.1, 0.5))
#define amp lj_band(0.0, 0.5)
#define iamp lj_band(0.0, 0.5)
#define iMouse vec4(RENDERSIZE * 0.5, 0.0, 0.0)
















const float PI = 3.1415926535897932384626433832795;

float pingPong(float x, float length) {
    float m = mod(x, length * 2.0);
    return m <= length ? m : length * 2.0 - m;
}

float hash21(vec2 p){
    p=fract(p*vec2(123.34,456.21));
    p+=dot(p,p+45.32);
    return fract(p.x*p.y);
}

vec2 hash22(vec2 p){
    float n=hash21(p);
    float m=hash21(p+17.7);
    return vec2(n,m);
}

vec3 hsv2rgb(vec3 c){
    vec4 K=vec4(1.0,2.0/3.0,1.0/3.0,3.0);
    vec3 p=abs(fract(c.xxx+K.xyz)*6.0-K.www);
    return c.z*mix(K.xxx,clamp(p-K.xxx,0.0,1.0),c.y);
}

float n2(vec2 p){
    vec2 i=floor(p),f=fract(p);
    vec2 u=f*f*(3.0-2.0*f);
    float a=hash21(i);
    float b=hash21(i+vec2(1,0));
    float c=hash21(i+vec2(0,1));
    float d=hash21(i+vec2(1,1));
    return mix(mix(a,b,u.x),mix(c,d,u.x),u.y);
}

float fbm(vec2 p){
    float s=0.0,a=0.5;
    for(int i=0;i<5;i++){
        s+=a*n2(p); p*=2.02; a*=0.5;
    }
    return s;
}

struct VOut{ float d; vec2 id; vec2 gv; vec2 nearest; };
VOut voronoi(vec2 p){
    vec2 i=floor(p), f=fract(p);
    float md=1e9; vec2 mid=vec2(0.0), mcell=vec2(0.0), mpt=vec2(0.0);
    for(int y=-1;y<=1;y++){
        for(int x=-1;x<=1;x++){
            vec2 g=vec2(x,y);
            vec2 id=i+g;
            vec2 rnd=hash22(id);
            vec2 o=sin((rnd*2.0-1.0)*6.28318 + vec2(TIME*0.35, TIME*0.27))*0.25 + rnd*0.5;
            vec2 r=g+o-f;
            float d=dot(r,r);
            if(d<md){md=d; mid=id; mcell=g; mpt=r;}
        }
    }
    float edge=1e9;
    for(int y=-1;y<=1;y++){
        for(int x=-1;x<=1;x++){
            vec2 g=vec2(x,y);
            if(all(equal(g,mcell))) continue;
            vec2 id=i+g;
            vec2 rnd=hash22(id);
            vec2 o=sin((rnd*2.0-1.0)*6.28318 + vec2(TIME*0.35, TIME*0.27))*0.25 + rnd*0.5;
            vec2 r=g+o-f;
            vec2 dif=0.5*(mpt+r);
            float d=(dot(dif,dif)-dot(0.5*(r-mpt),0.5*(r-mpt)))/max(length(dif),1e-4);
            edge=min(edge,abs(d));
        }
    }
    VOut v; v.d=sqrt(md); v.id=mid; v.gv=f; v.nearest=mpt; return VOut(edge,mid,f,mpt);
}

void main(){
    vec2 res=RENDERSIZE;
    float aspect=res.x/res.y;
    vec2 m=(iMouse.z>0.5)?iMouse.xy/res:vec2(0.5);
    vec2 uv=(isf_FragNormCoord-m)*vec2(aspect,1.0);

    float gzoom=exp(sin(TIME*0.6)*1.1);
    float zt=mix(0.65,1.85,0.5+0.5*sin(TIME*0.43));
    vec2 warp=vec2(fbm(uv*0.9+TIME*0.15), fbm(uv*0.9-vec2(3.1,1.7)+TIME*0.12));
    float localScale=mix(0.75,2.4,fbm(uv*0.6 + warp*0.8));
    vec2 p=uv*(gzoom*zt*localScale);

    vec2 flow=vec2(sin(TIME*0.23),cos(TIME*0.19))*0.6;
    p+=flow*fbm(uv*0.8+warp*0.5);

    VOut v=voronoi(p);

    float px=length(vec2(dFdx(isf_FragNormCoord.x),dFdy(isf_FragNormCoord.y)));
    float lead=0.006/(gzoom*localScale);
    float edge=1.0 - smoothstep(lead - px, lead + px, v.d);

    vec2 pid=v.id;
    float hue=fract(hash21(pid)*0.97 + 0.15*sin(TIME*0.2+hash21(pid+7.3)));
    float sat=mix(0.6,0.95,hash21(pid+3.7));
    float val=mix(0.7,1.0,hash21(pid+11.1));
    vec3 pane=hsv2rgb(vec3(hue,sat,val));

    vec2 nrm=normalize(v.nearest+1e-6);
    float gloss=0.55+0.45*pow(max(dot(nrm,normalize(vec2(cos(TIME*0.27),sin(TIME*0.31)))),0.0),1.2);
    pane*=gloss;

    vec3 base=texture(inputImage,isf_FragNormCoord).rgb;
    float baseMix=pingPong(0.12+0.15*hash21(pid+2.1) * (TIME * PI), 5.0);
    vec3 glass=mix(pane,base,baseMix);

    vec3 leadCol=vec3(0.08,0.09,0.10);
    vec3 col=mix(glass,leadCol,edge);

    float b=0.18;
    col+=pow(max(col-0.72,0.0),vec3(2.0))*b;

    float vign=1.0 - smoothstep(0.78,1.18,length((isf_FragNormCoord-m)*vec2(aspect,1.0)));
    col*=mix(0.92,1.12,vign);


    // --- Audio Reactivity: direct output modulation ---
    float _ab = clamp(amp_peak, 0.0, 1.0);
    float _abass = clamp(amp_low, 0.0, 1.0);
    col *= 1.0 + _ab * 0.6;
    col = mix(col, col * vec3(1.0 + _abass * 0.3, 1.0 - _abass * 0.15, 1.0 + clamp(amp_high, 0.0, 1.0) * 0.25), _ab);
    // --- End Audio Reactivity ---

    gl_FragColor=vec4(clamp(col,0.0,1.0),1.0);
}
