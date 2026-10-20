/*{
	"DESCRIPTION": "Glitches regions whose running temporal variance is high",
	"CREDIT": "by daito / Codex",
	"ISFVSN": "2",
	"CATEGORIES": [
		"Glitch",
		"Utility"
	],
	"INPUTS": [
		{
			"NAME": "inputImage",
			"TYPE": "image"
		},
		{
			"NAME": "memory",
			"LABEL": "Memory",
			"TYPE": "float",
			"MIN": 0.0,
			"MAX": 1.0,
			"DEFAULT": 0.94
		},
		{
			"NAME": "varianceGain",
			"LABEL": "Variance Gain",
			"TYPE": "float",
			"MIN": 0.0,
			"MAX": 10.0,
			"DEFAULT": 4.0
		},
		{
			"NAME": "threshold",
			"LABEL": "Threshold",
			"TYPE": "float",
			"MIN": 0.0,
			"MAX": 1.0,
			"DEFAULT": 0.18
		},
		{
			"NAME": "pixelSize",
			"LABEL": "Pixel Size",
			"TYPE": "float",
			"MIN": 1.0,
			"MAX": 80.0,
			"DEFAULT": 18.0
		},
		{
			"NAME": "posterize",
			"LABEL": "Posterize",
			"TYPE": "float",
			"MIN": 0.0,
			"MAX": 1.0,
			"DEFAULT": 0.45
		},
		{
			"NAME": "strobe",
			"LABEL": "Strobe",
			"TYPE": "float",
			"MIN": 0.0,
			"MAX": 1.0,
			"DEFAULT": 0.2
		},
		{
			"NAME": "resetNow",
			"LABEL": "Reset",
			"TYPE": "event"
		},
		{
			"NAME": "showVariance",
			"LABEL": "Show Variance",
			"TYPE": "bool",
			"DEFAULT": false
		}
	],
	"PASSES": [
		{
			"TARGET": "meanBuffer",
			"PERSISTENT": true,
			"FLOAT": true
		},
		{
			"TARGET": "varianceBuffer",
			"PERSISTENT": true,
			"FLOAT": true
		},
		{
			"TARGET": "glitchBuffer",
			"PERSISTENT": true
		},
		{
		
		}
	]
}*/

float rand(vec2 co)
{
	return fract(sin(dot(co.xy, vec2(12.9898, 78.233))) * 43758.5453);
}

void main()
{
	vec2 uv = isf_FragNormCoord.xy;
	bool doReset = ((FRAMEINDEX <= 1) || resetNow);
	vec4 inputPixel = IMG_THIS_NORM_PIXEL(inputImage);
	
	if (PASSINDEX == 0)	{
		if (doReset)	{
			gl_FragColor = inputPixel;
		}
		else	{
			gl_FragColor = mix(inputPixel, IMG_THIS_NORM_PIXEL(meanBuffer), memory);
		}
	}
	else if (PASSINDEX == 1)	{
		if (doReset)	{
			gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
		}
		else	{
			vec4 meanPixel = IMG_THIS_NORM_PIXEL(meanBuffer);
			float diff = length(inputPixel.rgb - meanPixel.rgb) * varianceGain;
			float oldVar = IMG_THIS_NORM_PIXEL(varianceBuffer).r;
			float variance = mix(diff, oldVar, memory);
			gl_FragColor = vec4(variance, variance, variance, 1.0);
		}
	}
	else if (PASSINDEX == 2)	{
		if (doReset)	{
			gl_FragColor = inputPixel;
		}
		else	{
			float variance = IMG_THIS_NORM_PIXEL(varianceBuffer).r;
			float active = smoothstep(threshold, threshold + 0.3, variance);
			vec2 cell = max(vec2(pixelSize) / RENDERSIZE.xy, vec2(1.0) / RENDERSIZE.xy);
			vec2 cellId = floor(uv / cell);
			vec2 cellUV = (cellId + vec2(0.5)) * cell;
			vec2 slip = (vec2(rand(cellId + vec2(TIME, 1.0)), rand(cellId + vec2(3.0, TIME))) - vec2(0.5)) * cell * active * 2.0;
			vec4 pix = IMG_NORM_PIXEL(inputImage, clamp(cellUV + slip, 0.0, 1.0));
			float levels = mix(256.0, 8.0, posterize * active);
			pix.rgb = floor(pix.rgb * levels) / levels;
			float blink = mix(1.0, step(0.35, rand(vec2(cellId.y, floor(TIME * 20.0)))), strobe * active);
			pix.rgb *= blink;
			gl_FragColor = mix(inputPixel, pix, active);
		}
	}
	else	{
		if (showVariance)	{
			gl_FragColor = IMG_THIS_NORM_PIXEL(varianceBuffer);
		}
		else	{
			gl_FragColor = IMG_THIS_NORM_PIXEL(glitchBuffer);
		}
	}
}
