/*{
  "DESCRIPTION": "(lostjared/shaders: mirror-zoom)",
  "CREDIT": "Jared Bruni (lostjared/shaders, MIT)",
  "ISFVSN": "2",
  "CATEGORIES": [
    "lostjared"
  ],
  "INPUTS": [
    {
      "NAME": "inputImage",
      "TYPE": "image"
    }
  ]
}*/

#define iMouse vec4(RENDERSIZE * 0.5, 0.0, 0.0)







mat3 rotX(float a){float s=sin(a),c=cos(a);return mat3(1,0,0, 0,c,-s, 0,s,c);}
mat3 rotY(float a){float s=sin(a),c=cos(a);return mat3(c,0,s, 0,1,0, -s,0,c);}
mat3 rotZ(float a){float s=sin(a),c=cos(a);return mat3(c,-s,0, s,c,0, 0,0,1);}

float pingPong(float x, float length) {
    float m = mod(x, length * 2.0);
    return m <= length ? m : length * 2.0 - m;
}

void main(void){

			const float PI = 3.1415926535897932384626433832795;

    float aspect=RENDERSIZE.x/RENDERSIZE.y;
    vec2 ar=vec2(aspect,1.0);
    vec2 m=(iMouse.z>0.5)?(iMouse.xy/RENDERSIZE):vec2(0.5);

    vec2 p=(isf_FragNormCoord-m)*ar;
    vec3 v=vec3(p,1.0);
    float ax=0.25*sin(TIME*0.7);
    float ay=0.25*cos(TIME*0.6);
    float az=0.4*TIME;
    mat3 R=rotZ(az)*rotY(ay)*rotX(ax);
    vec3 r=R*v;
    float persp=0.6;
    float zf=1.0/(1.0+r.z*persp);
    vec2 q=r.xy*zf;

    float eps=1e-6;
    float base=1.72;
    float period=log(base);
    float t=TIME*0.5;
    float rad=length(q)+eps;
    float ang=atan(q.y,q.x)+t*0.3;
    float k=fract((log(rad)-t)/period);
    float rw=exp(k*period);
    vec2 qwrap=vec2(cos(ang),sin(ang))*rw;

    float N=8.0;
    float stepA=6.28318530718/N;
    float a=atan(qwrap.y,qwrap.x)+TIME*0.05;
    a=mod(a,stepA);
    a=abs(a-stepA*0.5);
    vec2 kdir=vec2(cos(a),sin(a));
    vec2 kaleido=kdir*length(qwrap);
    kaleido = 1.0 - abs(1.0 - 2.0 * kaleido);
		 kaleido = fract(kaleido);
    vec2 uv=kaleido/ar+m;
    uv=fract(uv);
    gl_FragColor=texture(inputImage,sin(uv * (PI * pingPong(fract(TIME), 1.0))));
}